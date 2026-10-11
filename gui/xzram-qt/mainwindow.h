#ifndef MAINWINDOW_H
#define MAINWINDOW_H

#include "refreshrunner.h"

#include <QMainWindow>
#include <QString>

class DashboardWidget;
class DoctorWidget;
class PendingBanner;
class SettingsWidget;
class SnapshotWidget;
class SwapfileWidget;
class SysctlWidget;
class ZramWidget;
class QLabel;
class QTabWidget;
class QTimer;
class QJsonObject;
class QCloseEvent;

namespace LinkedOptimize {
class Runner;
}

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget *parent = nullptr);

protected:
    void closeEvent(QCloseEvent *event) override;

private slots:
    void refreshAll();
    void refreshLive();
    void applyPending();
    void clearPending();
    void onStagingChanged();
    void recommendDefaults();
    void onRefreshIntervalChanged(int intervalMs);
    void onPruneKeepDefaultChanged(int keep);
    void onLinkedOptimizeChanged(bool enabled);
    void onLinkedFieldEdited(const QString &anchor);
    void runLinkedOptimize();

private:
    void setupUi();
    void requestRefresh(bool full);
    void onRefreshFinished(const RefreshRunner::Results &results);
    void updatePendingBanner();
    void updateStatusLabel(bool daemonActive);
    void configureRefreshTimer(int intervalMs);
    void stageRecommendedDefaults(const QString &zramScale, const QString &swapScale,
                                  const QJsonObject &pending);
    void previewPendingInTabs(const QJsonObject &pending);
    bool pendingHasChanges(const QJsonObject &pending) const;
    PendingBanner *m_pendingBanner;
    QTabWidget *m_tabs;
    DashboardWidget *m_dashboard;
    ZramWidget *m_zramPage;
    SwapfileWidget *m_swapfilePage;
    SysctlWidget *m_sysctlPage;
    DoctorWidget *m_doctorPage;
    SnapshotWidget *m_snapshotPage;
    SettingsWidget *m_settingsPage;
    QLabel *m_statusLabel;
    QTimer *m_refreshTimer;
    QTimer *m_linkedOptimizeTimer;
    LinkedOptimize::Runner *m_linkedOptimizeRunner = nullptr;
    RefreshRunner *m_refresher = nullptr;
    bool m_fullRefreshQueued = false;
    QString m_pendingLinkedAnchor;
    bool m_linkedOptimizeEnabled = true;
    bool m_applyingLinkedOptimize = false;
};

#endif
