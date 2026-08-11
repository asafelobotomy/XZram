#ifndef SYSCTLWIDGET_H
#define SYSCTLWIDGET_H

#include <QJsonObject>
#include <QWidget>

class QPushButton;
class QSpinBox;

class SysctlWidget : public QWidget {
    Q_OBJECT

public:
    explicit SysctlWidget(QWidget *parent = nullptr);

    void setSysctlJson(const QString &json);

    QJsonObject pendingSeedFragment() const;
    void applyLinkedSysctl(const QJsonObject &sysctl);
    void setLinkedOptimizeBlocked(bool blocked);

signals:
    void stagingChanged();
    void linkedFieldEdited(const QString &anchor);

private slots:
    void applyZramDefaults();
    void stageChanges();
    void updateActionEnabled();
    void onSysctlEdited();

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
    bool m_linkedOptimizeBlocked = false;

    int m_baselineSwappiness = -1;
    int m_baselineBoost = -1;
    int m_baselineScale = -1;
    int m_baselinePageCluster = -1;
};

#endif
