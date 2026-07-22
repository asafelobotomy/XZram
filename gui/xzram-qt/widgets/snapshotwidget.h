#ifndef SNAPSHOTWIDGET_H
#define SNAPSHOTWIDGET_H

#include <QWidget>

class QTableWidget;
class QPushButton;
class QLabel;
class QLineEdit;
class QSpinBox;

class SnapshotWidget : public QWidget {
    Q_OBJECT

public:
    explicit SnapshotWidget(QWidget *parent = nullptr);
    void refresh();
    void setPruneKeepDefault(int keep);

signals:
    void configurationChanged();

private slots:
    void restoreSelected();
    void createSnapshot();
    void deleteSelected();
    void pruneSnapshots();
    void rollback();

private:
    void setupUi();
    QString selectedSnapshotId() const;

    QTableWidget *m_table;
    QLineEdit *m_labelEdit;
    QSpinBox *m_pruneKeepSpin;
    QPushButton *m_createButton;
    QPushButton *m_restoreButton;
    QPushButton *m_deleteButton;
    QPushButton *m_pruneButton;
    QPushButton *m_rollbackButton;
    QLabel *m_noteLabel;
};

#endif
