// MainWindow: asynchronous data refresh (never blocks the UI thread).
#include "mainwindow.h"

#include "xzramcli.h"
#include "widgets/dashboardwidget.h"
#include "widgets/doctorwidget.h"
#include "widgets/pendingbanner.h"
#include "widgets/settingswidget.h"
#include "widgets/snapshotwidget.h"
#include "widgets/swapfilewidget.h"
#include "widgets/sysctlwidget.h"
#include "widgets/zramwidget.h"

#include <QLabel>
#include <QTabWidget>

namespace {

using Q = RefreshRunner::Query;

const QList<Q> kFullQueries = {Q::Status,     Q::Detect,    Q::Doctor,  Q::Swaps,  Q::ZramConfig,
                               Q::Swapfiles,  Q::Sysctl,    Q::Pending, Q::Snapshots};
const QList<Q> kLiveQueries = {Q::Status, Q::Doctor, Q::Swaps, Q::Pending};

} // namespace

void MainWindow::updateStatusLabel(bool daemonActive) {
    const QString daemon = daemonActive ? tr("xzramd up") : tr("xzramd down");
    m_statusLabel->setText(
        tr("Using xzram CLI (%1) · %2").arg(XzramCli::findBinary(), daemon));
}

void MainWindow::updatePendingBanner() {
    m_pendingBanner->setPendingJson(XzramCli::pendingJson());
}

void MainWindow::refreshAll() {
    requestRefresh(true);
}

void MainWindow::refreshLive() {
    // Skip this tick if the previous refresh is still in flight.
    if (m_refresher->isRunning()) {
        return;
    }
    requestRefresh(false);
}

void MainWindow::requestRefresh(bool full) {
    if (m_refresher->isRunning()) {
        // A full refresh must not be lost; run it once the current batch completes.
        m_fullRefreshQueued = m_fullRefreshQueued || full;
        return;
    }
    QList<Q> queries = full ? kFullQueries : kLiveQueries;
    if (!full && m_tabs->currentWidget() == m_snapshotPage) {
        queries.append(Q::Snapshots);
    }
    m_refresher->start(queries);
}

void MainWindow::onRefreshFinished(const RefreshRunner::Results &results) {
    updateStatusLabel(results.daemonActive);
    m_settingsPage->setDaemonActive(results.daemonActive);

    // Per-widget order mirrors the original synchronous populate (some setters depend on it).
    const auto feed = [&results](Q query, const auto &setter) {
        if (results.has(query)) {
            setter(results.value(query));
        }
    };
    feed(Q::Status, [this](const QString &j) { m_dashboard->setStatusJson(j); });
    feed(Q::Swaps, [this](const QString &j) { m_dashboard->setSwapsJson(j); });
    feed(Q::Doctor, [this](const QString &j) { m_dashboard->setDoctorJson(j); });
    feed(Q::Detect, [this](const QString &j) { m_dashboard->setDetectionJson(j); });
    feed(Q::Status, [this](const QString &j) { m_zramPage->setStatusJson(j); });
    feed(Q::ZramConfig, [this](const QString &j) { m_zramPage->setZramConfigJson(j); });
    feed(Q::Detect, [this](const QString &j) { m_zramPage->setDetectionJson(j); });
    feed(Q::Swapfiles, [this](const QString &j) { m_swapfilePage->setSwapfilesJson(j); });
    feed(Q::Detect, [this](const QString &j) { m_swapfilePage->setDetectionJson(j); });
    feed(Q::Swaps, [this](const QString &j) { m_swapfilePage->setSwapsJson(j); });
    feed(Q::Sysctl, [this](const QString &j) { m_sysctlPage->setSysctlJson(j); });
    feed(Q::Detect, [this](const QString &j) { m_doctorPage->setDetectionJson(j); });
    feed(Q::Doctor, [this](const QString &j) { m_doctorPage->setDoctorJson(j); });
    feed(Q::Pending, [this](const QString &j) { m_pendingBanner->setPendingJson(j); });
    feed(Q::Snapshots, [this](const QString &j) { m_snapshotPage->setSnapshotsJson(j); });

    if (m_fullRefreshQueued) {
        m_fullRefreshQueued = false;
        requestRefresh(true);
    }
}
