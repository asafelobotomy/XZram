#ifndef DOCTORWIDGET_H
#define DOCTORWIDGET_H

#include <QWidget>

class DbusClient;
class QLabel;
class QVBoxLayout;

class DoctorWidget : public QWidget {
    Q_OBJECT

public:
    explicit DoctorWidget(DbusClient *client, QWidget *parent = nullptr);

    void setDoctorJson(const QString &json);

signals:
    void btrfsPrepared();

private:
    void clearIssues();
    QWidget *makeIssueCard(const QJsonObject &issue);

    DbusClient *m_client;
    QLabel *m_header;
    QVBoxLayout *m_issuesLayout;
    QWidget *m_issuesContainer;
};

#endif
