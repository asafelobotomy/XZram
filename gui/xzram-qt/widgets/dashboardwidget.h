#ifndef DASHBOARDWIDGET_H
#define DASHBOARDWIDGET_H

#include <QWidget>

class QLabel;
class QProgressBar;
class QPushButton;
class QTableWidget;

class DashboardWidget : public QWidget {
    Q_OBJECT

public:
    explicit DashboardWidget(QWidget *parent = nullptr);

    void setStatusJson(const QString &json);
    void setSwapsJson(const QString &json);
    void setDoctorJson(const QString &json);

signals:
    void recommendDefaultsRequested();

private:
    void clearState();
    void updateHealthChip(bool healthy, int issueCount);

    QLabel *m_memLabel;
    QProgressBar *m_memBar;
    QLabel *m_swapLabel;
    QProgressBar *m_swapBar;
    QLabel *m_zramCard;
    QTableWidget *m_swapTable;
    QLabel *m_healthChip;
    QPushButton *m_recommendButton;
};

#endif
