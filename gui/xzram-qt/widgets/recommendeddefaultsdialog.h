#ifndef RECOMMENDEDDEFAULTSDIALOG_H
#define RECOMMENDEDDEFAULTSDIALOG_H

#include <QDialog>
#include <QJsonObject>
#include <QString>

class RecommendedDefaultsScalePanel;

class RecommendedDefaultsDialog : public QDialog {
    Q_OBJECT

public:
    enum class Choice { Cancel, ApplyDefaults, Configure };

    struct Result {
        Choice choice = Choice::Cancel;
        QString zramScale = QStringLiteral("default");
        QString swapScale = QStringLiteral("default");
    };

    static Result showDialog(QWidget *parent, const QJsonObject &report);

private:
    explicit RecommendedDefaultsDialog(const QJsonObject &report, QWidget *parent = nullptr);

    QJsonObject m_report;
    RecommendedDefaultsScalePanel *m_scalePanel = nullptr;
};

#endif
