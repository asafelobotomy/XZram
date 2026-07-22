#ifndef SYSCTLWIDGET_H
#define SYSCTLWIDGET_H

#include <QWidget>

class QPushButton;
class QSpinBox;

class SysctlWidget : public QWidget {
    Q_OBJECT

public:
    explicit SysctlWidget(QWidget *parent = nullptr);

    void setSysctlJson(const QString &json);

signals:
    void stagingChanged();

private slots:
    void applyZramDefaults();
    void stageChanges();
    void updateActionEnabled();

private:
    void setSpinValue(QSpinBox *spin, const QJsonObject &obj, const QString &key);
    void captureBaseline();
    bool formDirty() const;

    QSpinBox *m_swappinessSpin;
    QSpinBox *m_boostSpin;
    QSpinBox *m_scaleSpin;
    QSpinBox *m_pageClusterSpin;
    QPushButton *m_defaultsButton;
    QPushButton *m_stageButton;

    int m_baselineSwappiness = -1;
    int m_baselineBoost = -1;
    int m_baselineScale = -1;
    int m_baselinePageCluster = -1;
};

#endif
