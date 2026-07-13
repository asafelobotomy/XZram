#ifndef SWAPFILEWIDGET_H
#define SWAPFILEWIDGET_H

#include <QWidget>

class QCheckBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QSpinBox;
class QTableWidget;
class DbusClient;

class SwapfileWidget : public QWidget {
    Q_OBJECT

public:
    explicit SwapfileWidget(DbusClient *client, QWidget *parent = nullptr);

    void setDaemonAvailable(bool available);
    void setSwapfilesJson(const QString &json);
    void setDetectionJson(const QString &json);

signals:
    void stagingChanged();

private slots:
    void browsePath();
    void stageCreate();
    void stageResize();
    void stageRemove();
    void checkBtrfs();
    void prepareBtrfs();

private:
    void setEditingEnabled(bool enabled);
    void populateTable(const QJsonArray &files);
    QString selectedPath() const;
    QString targetPath() const;
    void updateBtrfsStatus(const QString &json);
    QString fetchBtrfsCheckJson(const QString &path) const;

    DbusClient *m_client;
    bool m_daemonAvailable = false;
    bool m_onBtrfs = false;

    QLabel *m_btrfsBanner;
    QLabel *m_btrfsStatus;
    QCheckBox *m_mkdirCheck;
    QPushButton *m_checkBtrfsButton;
    QPushButton *m_prepareBtrfsButton;
    QLabel *m_unavailableLabel;
    QTableWidget *m_table;
    QLineEdit *m_pathEdit;
    QSpinBox *m_sizeSpin;
    QSpinBox *m_prioritySpin;
    QPushButton *m_browseButton;
    QPushButton *m_createButton;
    QPushButton *m_resizeButton;
    QPushButton *m_removeButton;
};

#endif
