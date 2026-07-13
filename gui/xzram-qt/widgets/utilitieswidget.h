#ifndef UTILITIESWIDGET_H
#define UTILITIESWIDGET_H

#include <QWidget>

class DbusClient;
class QTableWidget;
class QPushButton;
class QLabel;

class UtilitiesWidget : public QWidget {
    Q_OBJECT

public:
    explicit UtilitiesWidget(DbusClient *client, QWidget *parent = nullptr);
    void refresh();

private slots:
    void restoreSelected();

private:
    void setupUi();
    QString fetchSnapshotsJson() const;

    DbusClient *m_client;
    QTableWidget *m_table;
    QPushButton *m_restoreButton;
    QLabel *m_noteLabel;
};

#endif
