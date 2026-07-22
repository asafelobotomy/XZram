#include "mainwindow.h"

#include "jsonloader.h"
#include "xzramcli.h"
#include "widgets/dashboardwidget.h"
#include "widgets/doctorwidget.h"
#include "widgets/pendingbanner.h"
#include "widgets/recommendeddefaultsdialog.h"
#include "widgets/settingswidget.h"
#include "widgets/snapshotwidget.h"
#include "widgets/swapfilewidget.h"
#include "widgets/sysctlwidget.h"
#include "widgets/zramwidget.h"

#include <QHBoxLayout>
#include <QIcon>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QMessageBox>
#include <QTabWidget>
#include <QTimer>
#include <QVBoxLayout>
#include <QWidget>

MainWindow::MainWindow(QWidget *parent) : QMainWindow(parent) {
    m_refreshTimer = new QTimer(this);
    connect(m_refreshTimer, &QTimer::timeout, this, &MainWindow::refreshLive);
    setupUi();
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
    connect(m_doctorPage, &DoctorWidget::btrfsPrepared, this, &MainWindow::refreshAll);
    connect(m_snapshotPage, &SnapshotWidget::configurationChanged, this,
            &MainWindow::refreshAll);
    connect(m_dashboard, &DashboardWidget::recommendDefaultsRequested, this,
            &MainWindow::recommendDefaults);
    connect(m_settingsPage, &SettingsWidget::refreshIntervalChanged, this,
            &MainWindow::onRefreshIntervalChanged);
    connect(m_settingsPage, &SettingsWidget::pruneKeepDefaultChanged, this,
            &MainWindow::onPruneKeepDefaultChanged);
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

void MainWindow::updateStatusLabel() {
    const QString daemon =
        XzramCli::daemonIsActive() ? tr("xzramd up") : tr("xzramd down");
    m_statusLabel->setText(
        tr("Using xzram CLI (%1) · %2").arg(XzramCli::findBinary(), daemon));
}

void MainWindow::populateFromCli() {
    const QString status = XzramCli::statusJson();
    const QString detection = XzramCli::detectionJson();
    const QString doctor = XzramCli::doctorJson();
    const QString swaps = XzramCli::swapsJson();
    const QString zramConfig = XzramCli::zramConfigJson();
    const QString swapfiles = XzramCli::swapfilesJson();
    const QString sysctl = XzramCli::sysctlJson();
    const QString pending = XzramCli::pendingJson();

    m_dashboard->setStatusJson(status);
    m_dashboard->setSwapsJson(swaps);
    m_dashboard->setDoctorJson(doctor);
    m_dashboard->setDetectionJson(detection);
    m_zramPage->setStatusJson(status);
    m_zramPage->setZramConfigJson(zramConfig);
    m_zramPage->setDetectionJson(detection);
    m_swapfilePage->setSwapfilesJson(swapfiles);
    m_swapfilePage->setDetectionJson(detection);
    m_swapfilePage->setSwapsJson(swaps);
    m_sysctlPage->setSysctlJson(sysctl);
    m_doctorPage->setDetectionJson(detection);
    m_doctorPage->setDoctorJson(doctor);
    m_pendingBanner->setPendingJson(pending);
    m_settingsPage->refreshStatus();
}

void MainWindow::updatePendingBanner() {
    m_pendingBanner->setPendingJson(XzramCli::pendingJson());
}

void MainWindow::refreshAll() {
    updateStatusLabel();
    populateFromCli();
    m_snapshotPage->refresh();
}

void MainWindow::refreshLive() {
    updateStatusLabel();

    const QString status = XzramCli::statusJson();
    const QString doctor = XzramCli::doctorJson();
    const QString swaps = XzramCli::swapsJson();
    const QString pending = XzramCli::pendingJson();

    m_dashboard->setStatusJson(status);
    m_dashboard->setSwapsJson(swaps);
    m_dashboard->setDoctorJson(doctor);
    m_zramPage->setStatusJson(status);
    m_swapfilePage->setSwapsJson(swaps);
    m_doctorPage->setDoctorJson(doctor);
    m_pendingBanner->setPendingJson(pending);
    m_settingsPage->refreshStatus();

    if (m_tabs->currentWidget() == m_snapshotPage) {
        m_snapshotPage->refresh();
    }
}

void MainWindow::onStagingChanged() {
    updatePendingBanner();
}

void MainWindow::applyPending() {
    if (m_settingsPage->confirmBeforeApply()) {
        const auto answer = QMessageBox::question(
            this, tr("Apply pending"),
            tr("Apply staged configuration changes now?"),
            QMessageBox::Yes | QMessageBox::No, QMessageBox::Yes);
        if (answer != QMessageBox::Yes) {
            return;
        }
    }

    QString error;
    if (!XzramCli::apply(&error)) {
        QMessageBox::warning(this, tr("Apply failed"), error);
        return;
    }
    QMessageBox::information(this, tr("Apply"), tr("Pending configuration applied."));
    refreshAll();
}

void MainWindow::clearPending() {
    QString error;
    if (!XzramCli::clearPending(&error)) {
        QMessageBox::warning(this, tr("Clear failed"), error);
        return;
    }
    updatePendingBanner();
}

QString MainWindow::fetchRecommendedDefaultsJson() const {
    return XzramCli::recommendedDefaultsJson();
}

bool MainWindow::pendingHasChanges(const QJsonObject &pending) const {
    if (pending.isEmpty()) {
        return false;
    }
    if (pending.contains(QStringLiteral("error"))) {
        return false;
    }
    if (!pending.value(QStringLiteral("zram")).isNull()
        && pending.value(QStringLiteral("zram")).isObject()) {
        return true;
    }
    if (pending.value(QStringLiteral("disable_zram")).toBool()) {
        return true;
    }
    if (!pending.value(QStringLiteral("swapfile")).isNull()) {
        return true;
    }
    if (!pending.value(QStringLiteral("swapfile_resize")).isNull()) {
        return true;
    }
    if (!pending.value(QStringLiteral("remove_swapfile")).isNull()) {
        return true;
    }
    if (!pending.value(QStringLiteral("sysctl")).isNull()) {
        return true;
    }
    return false;
}

void MainWindow::previewPendingInTabs(const QJsonObject &pending) {
    if (pending.contains(QStringLiteral("zram"))) {
        const QJsonObject zram = pending.value(QStringLiteral("zram")).toObject();
        m_zramPage->setZramConfigJson(
            QString::fromUtf8(QJsonDocument(zram).toJson(QJsonDocument::Compact)));
    }
    if (pending.contains(QStringLiteral("sysctl"))) {
        const QJsonObject sysctl = pending.value(QStringLiteral("sysctl")).toObject();
        m_sysctlPage->setSysctlJson(
            QString::fromUtf8(QJsonDocument(sysctl).toJson(QJsonDocument::Compact)));
    }
}

void MainWindow::recommendDefaults() {
    const QString json = fetchRecommendedDefaultsJson();
    QString parseError;
    const QJsonObject report = JsonLoader::parseObject(json, &parseError);
    if (report.contains(QStringLiteral("error"))) {
        QMessageBox::warning(this, tr("Recommend failed"),
                             report.value(QStringLiteral("error")).toString());
        return;
    }

    const auto choice = RecommendedDefaultsDialog::showDialog(this, report);
    if (choice == RecommendedDefaultsDialog::Choice::Cancel) {
        return;
    }

    const QJsonObject pending = report.value(QStringLiteral("pending")).toObject();
    if (!pendingHasChanges(pending)) {
        QMessageBox::information(this, tr("Recommended defaults"),
                                 tr("Your system already matches the recommended defaults."));
        return;
    }

    if (choice == RecommendedDefaultsDialog::Choice::ApplyDefaults) {
        QString error;
        if (!XzramCli::defaultsApply(&error)) {
            QMessageBox::warning(this, tr("Apply failed"), error);
            return;
        }
        QMessageBox::information(this, tr("Apply complete"),
                                 tr("Recommended defaults have been applied."));
        refreshAll();
        return;
    }

    if (choice == RecommendedDefaultsDialog::Choice::Configure) {
        QString error;
        if (!XzramCli::defaultsStage(&error)) {
            QMessageBox::warning(this, tr("Stage failed"), error);
            return;
        }
        previewPendingInTabs(pending);
        updatePendingBanner();
        m_tabs->setCurrentWidget(m_zramPage);
        QMessageBox::information(
            this, tr("Defaults staged"),
            tr("Recommended defaults are staged. Review each tab, adjust if needed, then use "
               "Apply in the pending banner."));
        refreshAll();
    }
}
