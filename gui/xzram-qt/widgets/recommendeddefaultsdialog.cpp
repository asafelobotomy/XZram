#include "recommendeddefaultsdialog.h"

#include "formatutils.h"
#include "jsonloader.h"

#include <QDialogButtonBox>
#include <QFont>
#include <QFrame>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QLabel>
#include <QPushButton>
#include <QScrollArea>
#include <QSizePolicy>
#include <QVBoxLayout>

namespace {

QString profileLabel(const QString &profile) {
    const QString normalized = profile.toLower();
    if (normalized == QLatin1String("performance")) {
        return QStringLiteral("Performance");
    }
    if (normalized == QLatin1String("constrained")) {
        return QStringLiteral("Constrained");
    }
    return QStringLiteral("Conservative");
}

QString categoryTitle(const QString &category) {
    if (category == QLatin1String("zram")) {
        return QStringLiteral("ZRAM");
    }
    if (category == QLatin1String("sysctl")) {
        return QStringLiteral("Sysctl");
    }
    if (category == QLatin1String("swapfile")) {
        return QStringLiteral("Swap file");
    }
    return QStringLiteral("Advisory");
}

QString statusLabel(bool willStage, const QString &summary) {
    if (willStage) {
        return QStringLiteral("Will apply");
    }
    if (summary.contains(QStringLiteral("already matches"), Qt::CaseInsensitive)) {
        return QStringLiteral("Up to date");
    }
    return QStringLiteral("Advisory");
}

QLabel *makeChip(const QString &text, const QString &styleKey, QWidget *parent) {
    auto *chip = new QLabel(text, parent);
    chip->setAlignment(Qt::AlignCenter);
    chip->setObjectName(styleKey);
    chip->setSizePolicy(QSizePolicy::Maximum, QSizePolicy::Fixed);
    return chip;
}

QWidget *makeFactRow(const QString &title, const QString &value, QWidget *parent) {
    auto *row = new QWidget(parent);
    auto *layout = new QHBoxLayout(row);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(6);

    auto *titleLabel = new QLabel(title + QStringLiteral(":"), row);
    titleLabel->setObjectName(QStringLiteral("factTitle"));

    auto *valueLabel = new QLabel(value, row);
    valueLabel->setWordWrap(true);
    QFont valueFont = valueLabel->font();
    valueFont.setBold(true);
    valueLabel->setFont(valueFont);

    layout->addWidget(titleLabel);
    layout->addWidget(valueLabel, 1);
    return row;
}

QFrame *makeSummaryPanel(const QJsonObject &context, int stageCount, QWidget *parent) {
    auto *panel = new QFrame(parent);
    panel->setObjectName(QStringLiteral("recommendSummary"));
    panel->setFrameShape(QFrame::StyledPanel);

    auto *layout = new QVBoxLayout(panel);
    layout->setContentsMargins(14, 12, 14, 12);
    layout->setSpacing(8);

    auto *titleRow = new QHBoxLayout();
    const QString profile = profileLabel(JsonLoader::optionalString(context, QStringLiteral("profile")));
    auto *profileChip = makeChip(profile, QStringLiteral("profileChip"), panel);
    titleRow->addWidget(profileChip);

    titleRow->addStretch();

    if (stageCount > 0) {
        auto *changesChip = makeChip(
            QObject::tr("%n change(s) recommended", nullptr, stageCount), QStringLiteral("changesChip"),
            panel);
        titleRow->addWidget(changesChip);
    } else {
        auto *okChip = makeChip(QObject::tr("Already optimal"), QStringLiteral("okChip"), panel);
        titleRow->addWidget(okChip);
    }
    layout->addLayout(titleRow);

    auto *headline = new QLabel(panel);
    headline->setWordWrap(true);
    if (stageCount > 0) {
        headline->setText(QObject::tr(
            "Review the recommendations below. <b>Apply defaults</b> writes them immediately. "
            "<b>Stage for review</b> queues them so you can check each tab, then use Apply now "
            "in the pending banner."));
    } else {
        headline->setText(
            QObject::tr("Your system already matches the recommended defaults, or staging is "
                        "blocked on this host. You can still review advisory notes below. "
                        "Advisories are informational — Apply defaults does not clear them."));
    }
    headline->setObjectName(QStringLiteral("summaryHeadline"));
    layout->addWidget(headline);

    auto *facts = new QGridLayout();
    facts->setHorizontalSpacing(16);
    facts->setVerticalSpacing(4);

    const QString distro = JsonLoader::optionalString(context, QStringLiteral("distro"));
    const QString ram = FormatUtils::formatBytes(
        JsonLoader::optionalUInt64(context, QStringLiteral("mem_total_bytes")));
    const QString available = FormatUtils::formatBytes(
        JsonLoader::optionalUInt64(context, QStringLiteral("mem_available_bytes")));
    const QString backend = FormatUtils::humanizeEnum(
        JsonLoader::optionalString(context, QStringLiteral("zram_backend")));
    const QString rootFs =
        JsonLoader::optionalString(context, QStringLiteral("root_filesystem"));
    const bool hasDiskSwap =
        JsonLoader::optionalBool(context, QStringLiteral("has_disk_swap"), false);
    const bool hasZram =
        JsonLoader::optionalBool(context, QStringLiteral("has_active_zram"), false);
    const bool immutableOs =
        JsonLoader::optionalBool(context, QStringLiteral("immutable_os"), false);
    const bool etcWritable =
        JsonLoader::optionalBool(context, QStringLiteral("etc_writable"), true);

    facts->addWidget(makeFactRow(QObject::tr("System"), distro, panel), 0, 0);
    facts->addWidget(makeFactRow(QObject::tr("RAM"), ram, panel), 0, 1);
    facts->addWidget(makeFactRow(QObject::tr("Available"), available, panel), 1, 0);
    facts->addWidget(makeFactRow(QObject::tr("ZRAM backend"), backend, panel), 1, 1);
    facts->addWidget(
        makeFactRow(QObject::tr("Root FS"), rootFs.isEmpty() ? QObject::tr("Unknown") : rootFs,
                    panel),
        2, 0);
    facts->addWidget(
        makeFactRow(QObject::tr("Disk swap"),
                    hasDiskSwap ? QObject::tr("Present") : QObject::tr("None"), panel),
        2, 1);
    facts->addWidget(
        makeFactRow(QObject::tr("Active ZRAM"), hasZram ? QObject::tr("Yes") : QObject::tr("No"),
                    panel),
        3, 0);
    facts->addWidget(
        makeFactRow(QObject::tr("Writable /etc"),
                    etcWritable ? QObject::tr("Yes") : QObject::tr("No"), panel),
        3, 1);
    facts->addWidget(
        makeFactRow(QObject::tr("Immutable OS"),
                    immutableOs ? QObject::tr("Yes") : QObject::tr("No"), panel),
        4, 0);

    layout->addLayout(facts);
    return panel;
}

QFrame *makeRecommendationCard(const QJsonObject &item, QWidget *parent) {
    const bool willStage = JsonLoader::optionalBool(item, QStringLiteral("will_stage"), false);
    const QString category = JsonLoader::optionalString(item, QStringLiteral("category"));
    const QString summary = JsonLoader::optionalString(item, QStringLiteral("summary"));
    const QString detail = JsonLoader::optionalString(item, QStringLiteral("detail"));
    const QString reference = JsonLoader::optionalString(item, QStringLiteral("reference"));

    auto *card = new QFrame(parent);
    card->setObjectName(willStage ? QStringLiteral("recommendCardApply")
                                  : QStringLiteral("recommendCardInfo"));
    card->setFrameShape(QFrame::StyledPanel);

    auto *row = new QHBoxLayout(card);
    row->setContentsMargins(0, 0, 10, 0);
    row->setSpacing(10);

    auto *accent = new QFrame(card);
    accent->setObjectName(willStage ? QStringLiteral("cardAccentApply")
                                    : QStringLiteral("cardAccentInfo"));
    accent->setFixedWidth(4);
    row->addWidget(accent);

    auto *body = new QVBoxLayout();
    body->setContentsMargins(0, 10, 0, 10);
    body->setSpacing(6);

    auto *metaRow = new QHBoxLayout();
    metaRow->setSpacing(8);

    auto *categoryChip = makeChip(categoryTitle(category), QStringLiteral("categoryChip"), card);
    metaRow->addWidget(categoryChip);

    const QString status = statusLabel(willStage, summary);
    const QString statusKey =
        willStage ? QStringLiteral("statusApply") : QStringLiteral("statusInfo");
    metaRow->addWidget(makeChip(status, statusKey, card));
    metaRow->addStretch();
    body->addLayout(metaRow);

    auto *title = new QLabel(summary, card);
    title->setWordWrap(true);
    QFont titleFont = title->font();
    titleFont.setBold(true);
    titleFont.setPointSize(titleFont.pointSize() + 1);
    title->setFont(titleFont);
    body->addWidget(title);

    if (!detail.isEmpty()) {
        auto *detailLabel = new QLabel(detail, card);
        detailLabel->setWordWrap(true);
        detailLabel->setObjectName(QStringLiteral("cardDetail"));
        body->addWidget(detailLabel);
    }

    if (!reference.isEmpty()) {
        auto *refLabel = new QLabel(card);
        refLabel->setWordWrap(true);
        refLabel->setObjectName(QStringLiteral("cardReference"));
        refLabel->setText(QObject::tr("Reference: docs/RECOMMENDATIONS.md#%1").arg(reference));
        body->addWidget(refLabel);
    }

    row->addLayout(body, 1);
    return card;
}

void applyDialogStyle(QDialog *dialog) {
    dialog->setStyleSheet(QStringLiteral(
        "QDialog#recommendedDefaultsDialog { }"
        "QFrame#recommendSummary {"
        "  background-color: palette(base);"
        "  border: 1px solid palette(mid);"
        "  border-radius: 8px;"
        "}"
        "QLabel#summaryHeadline { color: palette(text); }"
        "QLabel#factTitle { color: palette(placeholderText); }"
        "QLabel#profileChip {"
        "  background-color: palette(highlight);"
        "  color: palette(highlighted-text);"
        "  border-radius: 10px;"
        "  padding: 4px 10px;"
        "  font-weight: bold;"
        "}"
        "QLabel#changesChip {"
        "  background-color: palette(link);"
        "  color: palette(base);"
        "  border-radius: 10px;"
        "  padding: 4px 10px;"
        "  font-weight: bold;"
        "}"
        "QLabel#okChip {"
        "  background-color: palette(midlight);"
        "  color: palette(text);"
        "  border-radius: 10px;"
        "  padding: 4px 10px;"
        "  font-weight: bold;"
        "}"
        "QFrame#recommendCardApply, QFrame#recommendCardInfo {"
        "  background-color: palette(alternate-base);"
        "  border: 1px solid palette(mid);"
        "  border-radius: 8px;"
        "}"
        "QFrame#cardAccentApply {"
        "  background-color: palette(link);"
        "  border-top-left-radius: 8px;"
        "  border-bottom-left-radius: 8px;"
        "}"
        "QFrame#cardAccentInfo {"
        "  background-color: palette(mid);"
        "  border-top-left-radius: 8px;"
        "  border-bottom-left-radius: 8px;"
        "}"
        "QLabel#categoryChip {"
        "  background-color: palette(button);"
        "  color: palette(button-text);"
        "  border: 1px solid palette(mid);"
        "  border-radius: 8px;"
        "  padding: 2px 8px;"
        "  font-size: 11px;"
        "  font-weight: bold;"
        "}"
        "QLabel#statusApply {"
        "  background-color: palette(link);"
        "  color: palette(base);"
        "  border-radius: 8px;"
        "  padding: 2px 8px;"
        "  font-size: 11px;"
        "  font-weight: bold;"
        "}"
        "QLabel#statusInfo {"
        "  background-color: palette(midlight);"
        "  color: palette(text);"
        "  border-radius: 8px;"
        "  padding: 2px 8px;"
        "  font-size: 11px;"
        "}"
        "QLabel#cardDetail { color: palette(placeholderText); }"
        "QLabel#cardReference { color: palette(placeholderText); font-size: 11px; }"));
}

} // namespace

RecommendedDefaultsDialog::RecommendedDefaultsDialog(const QJsonObject &report, QWidget *parent)
    : QDialog(parent), m_report(report) {
    setObjectName(QStringLiteral("recommendedDefaultsDialog"));
    setWindowTitle(tr("Recommended defaults"));
    resize(640, 520);
    applyDialogStyle(this);

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

    layout->addWidget(makeSummaryPanel(context, stageCount, this));

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
            listLayout->addWidget(makeRecommendationCard(item, listHost));
        }
    }

    if (!advisory.isEmpty()) {
        auto *sectionTitle = new QLabel(tr("Notes & advisories"), listHost);
        QFont sectionFont = sectionTitle->font();
        sectionFont.setBold(true);
        sectionTitle->setFont(sectionFont);
        listLayout->addWidget(sectionTitle);

        for (const QJsonObject &item : advisory) {
            listLayout->addWidget(makeRecommendationCard(item, listHost));
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
