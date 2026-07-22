#ifndef DOCTORWIDGET_H
#define DOCTORWIDGET_H

#include <QWidget>

class QLabel;
class QVBoxLayout;

class DoctorWidget : public QWidget {
    Q_OBJECT

public:
    explicit DoctorWidget(QWidget *parent = nullptr);

    void setDoctorJson(const QString &json);
    void setDetectionJson(const QString &json);

signals:
    void btrfsPrepared();

private:
    void clearIssues();
    QWidget *makeIssueCard(const QJsonObject &issue);

    QLabel *m_detectStrip;
    QLabel *m_header;
    QVBoxLayout *m_issuesLayout;
    QWidget *m_issuesContainer;
};

#endif
