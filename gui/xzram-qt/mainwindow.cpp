#include "mainwindow.h"

#include "clifallback.h"
#include "dbusclient.h"
#include "jsonloader.h"
#include "widgets/dashboardwidget.h"
#include "widgets/doctorwidget.h"
#include "widgets/pendingbanner.h"
#include "widgets/swapfilewidget.h"
#include "widgets/sysctlwidget.h"
#include "widgets/recommendeddefaultsdialog.h"
#include "widgets/utilitieswidget.h"
#include "widgets/zramwidget.h"

#include <QHBoxLayout>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QTabWidget>
#include <QVBoxLayout>
#include <QWidget>

MainWindow::MainWindow(QWidget *parent)
    : QMainWindow(parent), m_client(new DbusClient()) {
    setupUi();
    refreshAll();
}

void MainWindow::setupUi() {
    setWindowTitle(tr("XZram"));
    resize(960, 680);

    auto *central = new QWidget(this);
    auto *layout = new QVBoxLayout(central);

    auto *header = new QLabel(tr("XZram swap management"), central);
    layout->addWidget(header);

    m_statusLabel = new QLabel(central);
    m_statusLabel->setWordWrap(true);
    layout->addWidget(m_statusLabel);

    m_pendingBanner = new PendingBanner(central);
    layout->addWidget(m_pendingBanner);

    m_tabs = new QTabWidget(central);
    m_dashboard = new DashboardWidget(m_tabs);
    m_zramPage = new ZramWidget(m_client, m_tabs);
    m_swapfilePage = new SwapfileWidget(m_client, m_tabs);
    m_sysctlPage = new SysctlWidget(m_client, m_tabs);
    m_doctorPage = new DoctorWidget(m_client, m_tabs);
    m_utilitiesPage = new UtilitiesWidget(m_client, m_tabs);

    m_tabs->addTab(m_dashboard, tr("Dashboard"));
    m_tabs->addTab(m_zramPage, tr("ZRAM"));
    m_tabs->addTab(m_swapfilePage, tr("Swap Files"));
    m_tabs->addTab(m_sysctlPage, tr("Sysctl"));
    m_tabs->addTab(m_doctorPage, tr("Doctor"));
    m_tabs->addTab(m_utilitiesPage, tr("Utilities"));
    layout->addWidget(m_tabs, 1);

    auto *buttons = new QHBoxLayout();
    auto *refreshButton = new QPushButton(tr("Refresh"), central);
    m_startServiceButton = new QPushButton(tr("Start XZram service"), central);
    m_applyButton = new QPushButton(tr("Apply pending"), central);
    buttons->addWidget(refreshButton);
    buttons->addWidget(m_startServiceButton);
    buttons->addWidget(m_applyButton);
    buttons->addStretch();
    layout->addLayout(buttons);

    setCentralWidget(central);

    connect(refreshButton, &QPushButton::clicked, this, &MainWindow::refreshAll);
    connect(m_startServiceButton, &QPushButton::clicked, this, &MainWindow::startService);
    connect(m_applyButton, &QPushButton::clicked, this, &MainWindow::applyPending);
    connect(m_pendingBanner, &PendingBanner::applyRequested, this, &MainWindow::applyPending);
    connect(m_pendingBanner, &PendingBanner::clearRequested, this, &MainWindow::clearPending);
    connect(m_zramPage, &ZramWidget::stagingChanged, this, &MainWindow::onStagingChanged);
    connect(m_swapfilePage, &SwapfileWidget::stagingChanged, this, &MainWindow::onStagingChanged);
    connect(m_sysctlPage, &SysctlWidget::stagingChanged, this, &MainWindow::onStagingChanged);
    connect(m_doctorPage, &DoctorWidget::btrfsPrepared, this, &MainWindow::refreshAll);
    connect(m_dashboard, &DashboardWidget::recommendDefaultsRequested, this,
            &MainWindow::recommendDefaults);

    m_startServiceButton->setVisible(false);
    m_applyButton->setEnabled(false);
}

void MainWindow::setDaemonMode(bool available) {
    m_zramPage->setDaemonAvailable(available);
    m_swapfilePage->setDaemonAvailable(available);
    m_sysctlPage->setDaemonAvailable(available);
    m_pendingBanner->setDaemonAvailable(available);
    m_applyButton->setEnabled(available);
}

bool MainWindow::ensureBackend(QString *error) {
    m_usingCliFallback = false;
    m_statusLabel->setText(tr("Starting XZram service…"));
    m_startServiceButton->setVisible(false);

    QString dbusError;
    if (m_client->ensureAvailable(5000, &dbusError)) {
        m_statusLabel->clear();
        setDaemonMode(true);
        return true;
    }

    m_usingCliFallback = true;
    m_statusLabel->setText(
        tr("xzramd is not available. Showing read-only data from the xzram CLI.\n"
           "Click \"Start XZram service\" to enable staging and apply, or run: xzram daemon start"));
    m_startServiceButton->setVisible(true);
    setDaemonMode(false);
    if (error) {
        *error = dbusError;
    }
    return false;
}

void MainWindow::populateFromDbus() {
    const QString status = m_client->getStatusJson();
    const QString detection = m_client->getDetectionJson();
    const QString doctor = m_client->getDoctorJson();
    const QString swaps = m_client->listSwapsJson();
    const QString zramConfig = m_client->getZramConfigJson();
    const QString swapfiles = m_client->listSwapfilesJson();
    const QString sysctl = m_client->getSysctlJson();
    const QString pending = m_client->getPendingJson();

    m_dashboard->setStatusJson(status);
    m_dashboard->setSwapsJson(swaps);
    m_dashboard->setDoctorJson(doctor);
    m_zramPage->setStatusJson(status);
    m_zramPage->setZramConfigJson(zramConfig);
    m_zramPage->setDetectionJson(detection);
    m_swapfilePage->setSwapfilesJson(swapfiles);
    m_swapfilePage->setDetectionJson(detection);
    m_sysctlPage->setSysctlJson(sysctl);
    m_doctorPage->setDoctorJson(doctor);
    m_pendingBanner->setPendingJson(pending);
}

void MainWindow::populateFromCliFallback() {
    const QString status = CliFallback::statusJson();
    const QString detection = CliFallback::detectionJson();
    const QString doctor = CliFallback::doctorJson();
    const QString swaps = CliFallback::swapsJson();
    const QString zramConfig = CliFallback::zramConfigJson();
    const QString swapfiles = CliFallback::swapfilesJson();
    const QString sysctl = CliFallback::sysctlJson();

    m_dashboard->setStatusJson(status);
    m_dashboard->setSwapsJson(swaps);
    m_dashboard->setDoctorJson(doctor);
    m_zramPage->setStatusJson(status);
    m_zramPage->setZramConfigJson(zramConfig);
    m_zramPage->setDetectionJson(detection);
    m_swapfilePage->setSwapfilesJson(swapfiles);
    m_swapfilePage->setDetectionJson(detection);
    m_sysctlPage->setSysctlJson(sysctl);
    m_doctorPage->setDoctorJson(doctor);
    m_pendingBanner->hide();
}

void MainWindow::updatePendingBanner() {
    if (m_usingCliFallback) {
        m_pendingBanner->hide();
        return;
    }
    m_pendingBanner->setPendingJson(m_client->getPendingJson());
}

void MainWindow::refreshAll() {
    if (ensureBackend()) {
        QString snapError;
        if (!m_client->createSnapshot(QStringLiteral("app_open"), QString(), &snapError)) {
            m_statusLabel->setText(
                tr("Note: startup snapshot could not be created: %1").arg(snapError));
        }
        populateFromDbus();
        m_utilitiesPage->refresh();
        return;
    }
    populateFromCliFallback();
    m_utilitiesPage->refresh();
}

void MainWindow::onStagingChanged() {
    updatePendingBanner();
}

void MainWindow::startService() {
    m_statusLabel->setText(tr("Requesting administrator access to start xzramd…"));
    QString error;
    if (!m_client->startServiceViaHelper(&error)) {
        QMessageBox::warning(this, tr("Start failed"), error);
        m_statusLabel->setText(tr("Failed to start xzramd: %1").arg(error));
        return;
    }

    if (!m_client->ensureAvailable(5000, &error)) {
        QMessageBox::warning(this, tr("Start failed"), error);
        m_statusLabel->setText(tr("xzramd did not register on the system bus."));
        return;
    }

    m_usingCliFallback = false;
    m_startServiceButton->setVisible(false);
    m_statusLabel->setText(tr("XZram service is running."));
    setDaemonMode(true);
    populateFromDbus();
}

void MainWindow::applyPending() {
    if (m_usingCliFallback) {
        QMessageBox::information(
            this, tr("Apply unavailable"),
            tr("Apply requires xzramd. Click \"Start XZram service\" first."));
        return;
    }

    QString error;
    if (!m_client->applyPending(&error)) {
        QMessageBox::warning(this, tr("Apply failed"), error);
        return;
    }
    QMessageBox::information(this, tr("Apply"), tr("Pending configuration applied."));
    refreshAll();
}

void MainWindow::clearPending() {
    if (m_usingCliFallback) {
        return;
    }
    QString error;
    if (!m_client->clearPending(&error)) {
        QMessageBox::warning(this, tr("Clear failed"), error);
        return;
    }
    updatePendingBanner();
}

QString MainWindow::fetchRecommendedDefaultsJson() const {
    if (m_client->isRegistered()) {
        return m_client->getRecommendedDefaultsJson();
    }
    return CliFallback::recommendedDefaultsJson();
}

bool MainWindow::pendingHasChanges(const QJsonObject &pending) const {
    if (!pending.value(QStringLiteral("zram")).isNull()) {
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

bool MainWindow::stageRecommendedDefaults(QString *error) {
    if (m_usingCliFallback) {
        return m_client->stageRecommendedDefaults(error);
    }
    return m_client->stageRecommendedDefaults(error);
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
        if (!stageRecommendedDefaults(&error)) {
            QMessageBox::warning(this, tr("Stage failed"), error);
            return;
        }
        if (!m_client->applyPending(&error)) {
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
        if (!stageRecommendedDefaults(&error)) {
            QMessageBox::warning(this, tr("Stage failed"), error);
            return;
        }
        previewPendingInTabs(pending);
        updatePendingBanner();
        m_tabs->setCurrentWidget(m_zramPage);
        QMessageBox::information(
            this, tr("Defaults staged"),
            tr("Recommended defaults are staged. Review each tab, adjust if needed, then click "
               "Apply pending."));
        refreshAll();
    }
}
