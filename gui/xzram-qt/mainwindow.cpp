#include "mainwindow.h"

#include "linkedoptimize.h"
#include "privilegedjob.h"
#include "xzramcli.h"
#include "widgets/dashboardwidget.h"
#include "widgets/doctorwidget.h"
#include "widgets/pendingbanner.h"
#include "widgets/settingswidget.h"
#include "widgets/snapshotwidget.h"
#include "widgets/swapfilewidget.h"
#include "widgets/sysctlwidget.h"
#include "widgets/zramwidget.h"

#include <QCloseEvent>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QMessageBox>
#include <QMetaType>
#include <QTabWidget>
#include <QTimer>
#include <QVBoxLayout>
#include <QWidget>

MainWindow::MainWindow(QWidget *parent) : QMainWindow(parent) {
    qRegisterMetaType<XzramCli::RunResult>();
    m_refreshTimer = new QTimer(this);
    connect(m_refreshTimer, &QTimer::timeout, this, &MainWindow::refreshLive);
    m_linkedOptimizeTimer = new QTimer(this);
    m_linkedOptimizeTimer->setSingleShot(true);
    m_linkedOptimizeTimer->setInterval(200);
    connect(m_linkedOptimizeTimer, &QTimer::timeout, this, &MainWindow::runLinkedOptimize);
    m_linkedOptimizeRunner = new LinkedOptimize::Runner(this);
    m_refresher = new RefreshRunner(this);
    connect(m_refresher, &RefreshRunner::finished, this, &MainWindow::onRefreshFinished);
    setupUi();
    m_linkedOptimizeEnabled = m_settingsPage->linkedOptimize();
    configureRefreshTimer(m_settingsPage->refreshIntervalMs());
    refreshAll();
}

void MainWindow::setupUi() {
    setWindowTitle(tr("XZram"));
    setWindowIcon(QIcon(QStringLiteral(":/icons/xzram-icon.png")));
    resize(960, 680);

    auto *central = new QWidget(this);
    auto *layout = new QVBoxLayout(central);

    auto *headerRow = new QHBoxLayout();
    headerRow->addStretch(1);
    auto *iconLabel = new QLabel(central);
    iconLabel->setPixmap(
        QIcon(QStringLiteral(":/icons/xzram-icon.png")).pixmap(40, 40));
    auto *header = new QLabel(tr("XZram swap management"), central);
    QFont headerFont = header->font();
    headerFont.setPointSize(headerFont.pointSize() + 2);
    headerFont.setBold(true);
    header->setFont(headerFont);
    headerRow->addWidget(iconLabel);
    headerRow->addWidget(header);
    headerRow->addStretch(1);
    layout->addLayout(headerRow);

    m_statusLabel = new QLabel(central);
    m_statusLabel->setWordWrap(true);
    layout->addWidget(m_statusLabel);

    m_pendingBanner = new PendingBanner(central);
    layout->addWidget(m_pendingBanner);

    m_tabs = new QTabWidget(central);
    m_dashboard = new DashboardWidget(m_tabs);
    m_zramPage = new ZramWidget(m_tabs);
    m_swapfilePage = new SwapfileWidget(m_tabs);
    m_sysctlPage = new SysctlWidget(m_tabs);
    m_doctorPage = new DoctorWidget(m_tabs);
    m_snapshotPage = new SnapshotWidget(m_tabs);
    m_settingsPage = new SettingsWidget(m_tabs);

    m_tabs->addTab(m_dashboard, tr("Dashboard"));
    m_tabs->addTab(m_zramPage, tr("ZRAM"));
    m_tabs->addTab(m_swapfilePage, tr("Swap Files"));
    m_tabs->addTab(m_sysctlPage, tr("Sysctl"));
    m_tabs->addTab(m_doctorPage, tr("Doctor"));
    m_tabs->addTab(m_snapshotPage, tr("Snapshot"));
    m_tabs->addTab(m_settingsPage, tr("Settings"));
    layout->addWidget(m_tabs, 1);

    setCentralWidget(central);

    m_snapshotPage->setPruneKeepDefault(m_settingsPage->pruneKeepDefault());

    connect(m_pendingBanner, &PendingBanner::applyRequested, this, &MainWindow::applyPending);
    connect(m_pendingBanner, &PendingBanner::clearRequested, this, &MainWindow::clearPending);
    connect(m_zramPage, &ZramWidget::stagingChanged, this, &MainWindow::onStagingChanged);
    connect(m_swapfilePage, &SwapfileWidget::stagingChanged, this, &MainWindow::onStagingChanged);
    connect(m_swapfilePage, &SwapfileWidget::refreshRequested, this, &MainWindow::refreshAll);
    connect(m_sysctlPage, &SysctlWidget::stagingChanged, this, &MainWindow::onStagingChanged);
    connect(m_zramPage, &ZramWidget::linkedFieldEdited, this, &MainWindow::onLinkedFieldEdited);
    connect(m_sysctlPage, &SysctlWidget::linkedFieldEdited, this, &MainWindow::onLinkedFieldEdited);
    connect(m_swapfilePage, &SwapfileWidget::linkedFieldEdited, this,
            &MainWindow::onLinkedFieldEdited);
    connect(m_doctorPage, &DoctorWidget::btrfsPrepared, this, &MainWindow::refreshAll);
    connect(m_snapshotPage, &SnapshotWidget::configurationChanged, this,
            &MainWindow::refreshAll);
    connect(m_dashboard, &DashboardWidget::recommendDefaultsRequested, this,
            &MainWindow::recommendDefaults);
    connect(m_settingsPage, &SettingsWidget::refreshIntervalChanged, this,
            &MainWindow::onRefreshIntervalChanged);
    connect(m_settingsPage, &SettingsWidget::pruneKeepDefaultChanged, this,
            &MainWindow::onPruneKeepDefaultChanged);
    connect(m_settingsPage, &SettingsWidget::linkedOptimizeChanged, this,
            &MainWindow::onLinkedOptimizeChanged);
}

void MainWindow::configureRefreshTimer(int intervalMs) {
    m_refreshTimer->stop();
    if (intervalMs > 0) {
        m_refreshTimer->start(intervalMs);
    }
}

void MainWindow::onRefreshIntervalChanged(int intervalMs) {
    configureRefreshTimer(intervalMs);
}

void MainWindow::onPruneKeepDefaultChanged(int keep) {
    m_snapshotPage->setPruneKeepDefault(keep);
}

void MainWindow::onLinkedOptimizeChanged(bool enabled) {
    m_linkedOptimizeEnabled = enabled;
    if (!enabled) {
        m_pendingLinkedAnchor.clear();
        m_linkedOptimizeTimer->stop();
        m_linkedOptimizeRunner->cancel();
    }
}

void MainWindow::onLinkedFieldEdited(const QString &anchor) {
    if (!m_linkedOptimizeEnabled || m_applyingLinkedOptimize) {
        return;
    }
    m_pendingLinkedAnchor = anchor;
    m_linkedOptimizeTimer->start();
}

void MainWindow::runLinkedOptimize() {
    if (!m_linkedOptimizeEnabled || m_pendingLinkedAnchor.isEmpty() || m_applyingLinkedOptimize) {
        return;
    }
    const QString anchor = m_pendingLinkedAnchor;
    m_pendingLinkedAnchor.clear();
    const QString seed =
        LinkedOptimize::gatherSeedJson(m_zramPage, m_sysctlPage, m_swapfilePage);
    m_linkedOptimizeRunner->start(
        anchor, seed, m_zramPage, m_sysctlPage, m_swapfilePage, m_statusLabel,
        [this](bool applying) { m_applyingLinkedOptimize = applying; },
        [this]() {
            if (!m_pendingLinkedAnchor.isEmpty()) {
                m_linkedOptimizeTimer->start();
            }
        });
}

void MainWindow::closeEvent(QCloseEvent *event) {
    if (privilegedJobsRunning() > 0) {
        // Closing would kill the CLI mid-operation and orphan pkexec/xzram-helper.
        QMessageBox::information(this, tr("Operation in progress"),
                                 tr("Wait for the current privileged operation to finish before "
                                    "closing XZram."));
        event->ignore();
        return;
    }
    QMainWindow::closeEvent(event);
}
