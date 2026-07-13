#ifndef RECOMMENDEDDEFAULTSDIALOG_H
#define RECOMMENDEDDEFAULTSDIALOG_H

#include <QDialog>
#include <QJsonObject>

class RecommendedDefaultsDialog : public QDialog {
    Q_OBJECT

public:
    enum class Choice { Cancel, ApplyDefaults, Configure };

    static Choice showDialog(QWidget *parent, const QJsonObject &report);

private:
    explicit RecommendedDefaultsDialog(const QJsonObject &report, QWidget *parent = nullptr);

    QJsonObject m_report;
};

#endif
