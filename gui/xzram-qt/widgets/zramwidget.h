#ifndef ZRAMWIDGET_H
#define ZRAMWIDGET_H

#include <QJsonObject>
#include <QWidget>

class QComboBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QSpinBox;

class ZramWidget : public QWidget {
    Q_OBJECT

public:
    explicit ZramWidget(QWidget *parent = nullptr);

    void setStatusJson(const QString &json);
    void setZramConfigJson(const QString &json);
    void setDetectionJson(const QString &json);

    QJsonObject pendingSeedFragment() const;
    void applyLinkedZram(const QJsonObject &zram);
    void setLinkedOptimizeBlocked(bool blocked);

signals:
    void stagingChanged();
    void linkedFieldEdited(const QString &anchor);

private slots:
    void stageChanges();
    void disableZram();
    void migrateZram();
    void updateActionEnabled();
    void onSizeEdited();
    void onAlgoEdited();
    void onPriorityEdited();

private:
    void updateLiveStats(const QJsonObject &status);
    void updateConfigForm(const QJsonValue &config, bool resetBaseline = true);
    void updateMismatchWarning();
    void captureBaseline();
    bool formDirty() const;

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
    bool m_hasActiveZram = false;
    bool m_linkedOptimizeBlocked = false;

    QString m_baselineDevice;
    QString m_baselineSize;
    QString m_baselineAlgo;
    int m_baselinePriority = 100;
};

#endif
