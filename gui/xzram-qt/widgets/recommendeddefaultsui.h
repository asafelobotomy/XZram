#ifndef RECOMMENDEDDEFAULTSUI_H
#define RECOMMENDEDDEFAULTSUI_H

#include <QString>

class QDialog;
class QFrame;
class QJsonObject;
class QLabel;
class QWidget;

namespace RecommendedDefaultsUi {

QString profileLabel(const QString &profile);
QString categoryTitle(const QString &category);
QString statusLabel(bool willStage, const QString &summary);

QLabel *makeChip(const QString &text, const QString &styleKey, QWidget *parent);
QWidget *makeFactRow(const QString &title, const QString &value, QWidget *parent);
QFrame *makeSummaryPanel(const QJsonObject &context, int stageCount, QWidget *parent);
QFrame *makeRecommendationCard(const QJsonObject &item, QWidget *parent);
void applyDialogStyle(QDialog *dialog);

} // namespace RecommendedDefaultsUi

#endif
