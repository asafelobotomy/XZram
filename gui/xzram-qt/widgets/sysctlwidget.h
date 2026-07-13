#ifndef SYSCTLWIDGET_H
#define SYSCTLWIDGET_H

#include <QWidget>

class QLabel;
class QPushButton;
class QSpinBox;
class DbusClient;

class SysctlWidget : public QWidget {
    Q_OBJECT

public:
    explicit SysctlWidget(DbusClient *client, QWidget *parent = nullptr);

    void setDaemonAvailable(bool available);
    void setSysctlJson(const QString &json);

signals:
    void stagingChanged();

private slots:
    void applyZramDefaults();
    void stageChanges();

private:
    void setEditingEnabled(bool enabled);
    void setSpinValue(QSpinBox *spin, const QJsonObject &obj, const QString &key);

    DbusClient *m_client;
    bool m_daemonAvailable = false;

    QLabel *m_unavailableLabel;
    QSpinBox *m_swappinessSpin;
    QSpinBox *m_boostSpin;
    QSpinBox *m_scaleSpin;
    QSpinBox *m_pageClusterSpin;
    QPushButton *m_defaultsButton;
    QPushButton *m_stageButton;
};

#endif
