#ifndef MAINWINDOW_H
#define MAINWINDOW_H

#include <QMainWindow>

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

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget *parent = nullptr);

private slots:
    void refreshAll();
    void refreshLive();
    void applyPending();
    void clearPending();
    void onStagingChanged();
    void recommendDefaults();
    void onRefreshIntervalChanged(int intervalMs);
    void onPruneKeepDefaultChanged(int keep);

private:
    void setupUi();
    void populateFromCli();
    void updatePendingBanner();
    void updateStatusLabel();
    void configureRefreshTimer(int intervalMs);
    QString fetchRecommendedDefaultsJson() const;
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
};

#endif
