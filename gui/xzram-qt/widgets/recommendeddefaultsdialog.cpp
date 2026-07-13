#include "recommendeddefaultsdialog.h"

#include "formatutils.h"
#include "jsonloader.h"

#include <QDialogButtonBox>
#include <QJsonArray>
#include <QLabel>
#include <QPushButton>
#include <QScrollArea>
#include <QVBoxLayout>

RecommendedDefaultsDialog::RecommendedDefaultsDialog(const QJsonObject &report, QWidget *parent)
    : QDialog(parent), m_report(report) {
    setWindowTitle(tr("Recommended defaults"));
    resize(560, 420);

    auto *layout = new QVBoxLayout(this);

    const QJsonObject context = report.value(QStringLiteral("context")).toObject();
    auto *intro = new QLabel(this);
    intro->setWordWrap(true);
    intro->setText(
        tr("<b>Recommended defaults for your system</b><br>"
           "%1 · %2 RAM · ZRAM backend: %3")
            .arg(JsonLoader::optionalString(context, QStringLiteral("distro")),
                 FormatUtils::formatBytes(
                     JsonLoader::optionalUInt64(context, QStringLiteral("mem_total_bytes"))),
                 FormatUtils::humanizeEnum(
                     JsonLoader::optionalString(context, QStringLiteral("zram_backend")))));
    layout->addWidget(intro);

    auto *scroll = new QScrollArea(this);
    scroll->setWidgetResizable(true);
    auto *listHost = new QWidget(scroll);
    auto *listLayout = new QVBoxLayout(listHost);

    const QJsonArray items = report.value(QStringLiteral("items")).toArray();
    for (const QJsonValue &value : items) {
        const QJsonObject item = value.toObject();
        const bool willStage = JsonLoader::optionalBool(item, QStringLiteral("will_stage"), false);
        const QString category = JsonLoader::optionalString(item, QStringLiteral("category"));
        const QString summary = JsonLoader::optionalString(item, QStringLiteral("summary"));
        const QString detail = JsonLoader::optionalString(item, QStringLiteral("detail"));

        auto *card = new QLabel(listHost);
        card->setWordWrap(true);
        const QString badge =
            willStage ? tr("<span style='color:#155724'>[will stage]</span>")
                      : tr("<span style='color:#0c5460'>[info]</span>");
        const QString reference = JsonLoader::optionalString(item, QStringLiteral("reference"));
        QString referenceLine;
        if (!reference.isEmpty()) {
            referenceLine =
                QStringLiteral("<br><small>ref: docs/RECOMMENDATIONS.md#%1</small>").arg(reference);
        }
        card->setText(QStringLiteral("%1 <b>%2</b> — %3<br><i>%4</i>%5")
                          .arg(badge, FormatUtils::humanizeEnum(category), summary, detail,
                               referenceLine));
        card->setStyleSheet(QStringLiteral(
            "padding: 8px; border-radius: 6px; background: #f8f9fa; margin-bottom: 4px;"));
        listLayout->addWidget(card);
    }
    listLayout->addStretch();
    scroll->setWidget(listHost);
    layout->addWidget(scroll, 1);

    auto *buttons = new QDialogButtonBox(this);
    auto *applyButton = buttons->addButton(tr("Apply defaults"), QDialogButtonBox::AcceptRole);
    auto *configureButton = buttons->addButton(tr("Configure"), QDialogButtonBox::ActionRole);
    auto *cancelButton = buttons->addButton(QDialogButtonBox::Cancel);
    applyButton->setDefault(true);
    layout->addWidget(buttons);

    connect(applyButton, &QPushButton::clicked, this, [this]() {
        done(static_cast<int>(Choice::ApplyDefaults));
    });
    connect(configureButton, &QPushButton::clicked, this, [this]() {
        done(static_cast<int>(Choice::Configure));
    });
    connect(cancelButton, &QPushButton::clicked, this, [this]() {
        done(static_cast<int>(Choice::Cancel));
    });
}

RecommendedDefaultsDialog::Choice RecommendedDefaultsDialog::showDialog(QWidget *parent,
                                                                      const QJsonObject &report) {
    RecommendedDefaultsDialog dialog(report, parent);
    return static_cast<Choice>(dialog.exec());
}
