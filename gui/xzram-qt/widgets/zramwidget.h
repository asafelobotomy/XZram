#ifndef ZRAMWIDGET_H
#define ZRAMWIDGET_H

#include <QWidget>

class QComboBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QSpinBox;
class DbusClient;

class ZramWidget : public QWidget {
    Q_OBJECT

public:
    explicit ZramWidget(DbusClient *client, QWidget *parent = nullptr);

    void setDaemonAvailable(bool available);
    void setStatusJson(const QString &json);
    void setZramConfigJson(const QString &json);
    void setDetectionJson(const QString &json);

signals:
    void stagingChanged();

private slots:
    void stageChanges();
    void disableZram();
    void migrateZram();

private:
    void updateLiveStats(const QJsonObject &status);
    void updateConfigForm(const QJsonValue &config);
    void updateMismatchWarning();
    void setEditingEnabled(bool enabled);

    DbusClient *m_client;
    bool m_daemonAvailable = false;

    QLabel *m_statsLabel;
    QLabel *m_mismatchWarning;
    QLineEdit *m_deviceEdit;
    QLineEdit *m_sizeEdit;
    QLineEdit *m_residentLimitEdit;
    QComboBox *m_algoCombo;
    QSpinBox *m_prioritySpin;
    QPushButton *m_stageButton;
    QPushButton *m_disableButton;
    QPushButton *m_migrateButton;
    QString m_activeAlgorithm;
};

#endif
