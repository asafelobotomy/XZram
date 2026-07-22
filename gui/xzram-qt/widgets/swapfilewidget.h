#ifndef SWAPFILEWIDGET_H
#define SWAPFILEWIDGET_H

#include <QWidget>

class QCheckBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QSpinBox;
class QTableWidget;

class SwapfileWidget : public QWidget {
    Q_OBJECT

public:
    explicit SwapfileWidget(QWidget *parent = nullptr);

    void setSwapfilesJson(const QString &json);
    void setDetectionJson(const QString &json);
    void setSwapsJson(const QString &json);

signals:
    void stagingChanged();
    void refreshRequested();

private slots:
    void browsePath();
    void stageCreate();
    void stageResize();
    void stageRemove();
    void checkBtrfs();
    void prepareBtrfs();
    void swapOnSelected();
    void swapOffSelected();

private:
    void populateTable(const QJsonArray &files);
    void populatePartitionTable(const QJsonArray &swaps);
    QString selectedPath() const;
    QString selectedPartitionDevice() const;
    QString targetPath() const;
    void updateBtrfsStatus(const QString &json);
    void updateBtrfsBanner();
    bool anySwapfileReady() const;
    void updateActionEnabled();
    void captureCreateBaseline();
    bool createFormDirty() const;

    bool m_onBtrfs = false;

    QLabel *m_introLabel;
    QLabel *m_btrfsBanner;
    QLabel *m_btrfsStatus;
    QCheckBox *m_mkdirCheck;
    QPushButton *m_checkBtrfsButton;
    QPushButton *m_prepareBtrfsButton;
    QTableWidget *m_table;
    QTableWidget *m_partitionTable;
    QLineEdit *m_pathEdit;
    QSpinBox *m_sizeSpin;
    QSpinBox *m_prioritySpin;
    QPushButton *m_browseButton;
    QPushButton *m_createButton;
    QPushButton *m_resizeButton;
    QPushButton *m_removeButton;
    QPushButton *m_swapOnButton;
    QPushButton *m_swapOffButton;

    QString m_baselineCreatePath;
    quint64 m_baselineCreateSizeMb = 0;
    int m_baselineCreatePriority = 10;
};

#endif
