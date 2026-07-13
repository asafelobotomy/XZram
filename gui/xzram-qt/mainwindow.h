#ifndef MAINWINDOW_H
#define MAINWINDOW_H

#include <QMainWindow>

class DashboardWidget;
class DoctorWidget;
class DbusClient;
class PendingBanner;
class SwapfileWidget;
class SysctlWidget;
class UtilitiesWidget;
class ZramWidget;
class QPushButton;
class QLabel;
class QTabWidget;

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget *parent = nullptr);

private slots:
    void refreshAll();
    void applyPending();
    void clearPending();
    void startService();
    void onStagingChanged();
    void recommendDefaults();

private:
    void setupUi();
    bool ensureBackend(QString *error = nullptr);
    void populateFromDbus();
    void populateFromCliFallback();
    void updatePendingBanner();
    void setDaemonMode(bool available);
    QString fetchRecommendedDefaultsJson() const;
    void previewPendingInTabs(const QJsonObject &pending);
    bool stageRecommendedDefaults(QString *error = nullptr);
    bool pendingHasChanges(const QJsonObject &pending) const;

    DbusClient *m_client;
    PendingBanner *m_pendingBanner;
    QTabWidget *m_tabs;
    DashboardWidget *m_dashboard;
    ZramWidget *m_zramPage;
    SwapfileWidget *m_swapfilePage;
    SysctlWidget *m_sysctlPage;
    DoctorWidget *m_doctorPage;
    UtilitiesWidget *m_utilitiesPage;
    QPushButton *m_applyButton;
    QPushButton *m_startServiceButton;
    QLabel *m_statusLabel;
    bool m_usingCliFallback = false;
};

#endif
