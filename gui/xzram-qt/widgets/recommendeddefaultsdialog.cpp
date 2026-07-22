#include "recommendeddefaultsdialog.h"

#include "jsonloader.h"
#include "recommendeddefaultsui.h"

#include <QDialogButtonBox>
#include <QFont>
#include <QFrame>
#include <QJsonArray>
#include <QLabel>
#include <QPushButton>
#include <QScrollArea>
#include <QVBoxLayout>

RecommendedDefaultsDialog::RecommendedDefaultsDialog(const QJsonObject &report, QWidget *parent)
    : QDialog(parent), m_report(report) {
    setObjectName(QStringLiteral("recommendedDefaultsDialog"));
    setWindowTitle(tr("Recommended defaults"));
    resize(640, 520);
    RecommendedDefaultsUi::applyDialogStyle(this);

    auto *layout = new QVBoxLayout(this);
    layout->setSpacing(12);

    const QJsonObject context = report.value(QStringLiteral("context")).toObject();
    const QJsonArray items = report.value(QStringLiteral("items")).toArray();

    int stageCount = 0;
    for (const QJsonValue &value : items) {
        if (JsonLoader::optionalBool(value.toObject(), QStringLiteral("will_stage"), false)) {
            ++stageCount;
        }
    }

    layout->addWidget(RecommendedDefaultsUi::makeSummaryPanel(context, stageCount, this));

    auto *scroll = new QScrollArea(this);
    scroll->setWidgetResizable(true);
    scroll->setFrameShape(QFrame::NoFrame);
    auto *listHost = new QWidget(scroll);
    auto *listLayout = new QVBoxLayout(listHost);
    listLayout->setContentsMargins(0, 0, 0, 0);
    listLayout->setSpacing(10);

    QList<QJsonObject> staged;
    QList<QJsonObject> advisory;
    for (const QJsonValue &value : items) {
        const QJsonObject item = value.toObject();
        if (JsonLoader::optionalBool(item, QStringLiteral("will_stage"), false)) {
            staged.append(item);
        } else {
            advisory.append(item);
        }
    }

    if (!staged.isEmpty()) {
        auto *sectionTitle = new QLabel(tr("Changes to apply"), listHost);
        QFont sectionFont = sectionTitle->font();
        sectionFont.setBold(true);
        sectionTitle->setFont(sectionFont);
        listLayout->addWidget(sectionTitle);

        for (const QJsonObject &item : staged) {
            listLayout->addWidget(RecommendedDefaultsUi::makeRecommendationCard(item, listHost));
        }
    }

    if (!advisory.isEmpty()) {
        auto *sectionTitle = new QLabel(tr("Notes & advisories"), listHost);
        QFont sectionFont = sectionTitle->font();
        sectionFont.setBold(true);
        sectionTitle->setFont(sectionFont);
        listLayout->addWidget(sectionTitle);

        for (const QJsonObject &item : advisory) {
            listLayout->addWidget(RecommendedDefaultsUi::makeRecommendationCard(item, listHost));
        }
    }

    listLayout->addStretch();
    scroll->setWidget(listHost);
    layout->addWidget(scroll, 1);

    auto *buttons = new QDialogButtonBox(this);
    auto *applyButton = buttons->addButton(tr("Apply defaults"), QDialogButtonBox::AcceptRole);
    auto *configureButton = buttons->addButton(tr("Stage for review"), QDialogButtonBox::ActionRole);
    auto *cancelButton = buttons->addButton(QDialogButtonBox::Cancel);
    applyButton->setDefault(true);
    applyButton->setEnabled(stageCount > 0);
    configureButton->setEnabled(stageCount > 0);
    cancelButton->setToolTip(tr("Close without changing anything."));
    if (stageCount > 0) {
        applyButton->setToolTip(
            tr("Write the recommended settings to the system now (may ask for admin permission)."));
        configureButton->setToolTip(
            tr("Queue the recommendations so you can review each tab, then Apply now in the banner."));
    } else {
        applyButton->setToolTip(tr("No recommended changes to apply right now."));
        configureButton->setToolTip(tr("Nothing to queue for review right now."));
    }
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
