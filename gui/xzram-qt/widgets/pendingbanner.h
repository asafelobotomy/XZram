#ifndef PENDINGBANNER_H
#define PENDINGBANNER_H

#include <QWidget>

class QLabel;
class QPushButton;

class PendingBanner : public QWidget {
    Q_OBJECT

public:
    explicit PendingBanner(QWidget *parent = nullptr);

    void setPendingJson(const QString &json);
    void setDaemonAvailable(bool available);

signals:
    void applyRequested();
    void clearRequested();

private:
    QLabel *m_label;
    QPushButton *m_applyButton;
    QPushButton *m_clearButton;
};

#endif
