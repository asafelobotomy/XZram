#include "recommendeddefaultsscale.h"

#include "jsonloader.h"

#include <QFont>
#include <QHBoxLayout>
#include <QJsonObject>
#include <QLabel>
#include <QSlider>
#include <QVBoxLayout>

RecommendedDefaultsScalePanel::RecommendedDefaultsScalePanel(const QJsonObject &sizeScales,
                                                             QWidget *parent)
    : QWidget(parent) {
    setObjectName(QStringLiteral("recommendScalePanel"));
    m_zramGroup = sizeScales.value(QStringLiteral("zram")).toObject();
    m_swapGroup = sizeScales.value(QStringLiteral("swapfile")).toObject();

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 4, 0, 4);
    layout->setSpacing(10);

    auto *title = new QLabel(tr("Size scale"), this);
    QFont titleFont = title->font();
    titleFont.setBold(true);
    title->setFont(titleFont);
    layout->addWidget(title);

    auto *hint = new QLabel(
        tr("Choose Low, Recommended, or High before applying. Preview updates as you slide."),
        this);
    hint->setWordWrap(true);
    hint->setObjectName(QStringLiteral("scaleHint"));
    layout->addWidget(hint);

    auto addRow = [this, layout](const QString &labelText, QSlider **sliderOut, QLabel **previewOut,
                                 const QJsonObject &group, bool isZram, bool enabled) {
        auto *row = new QWidget(this);
        auto *rowLayout = new QVBoxLayout(row);
        rowLayout->setContentsMargins(0, 0, 0, 0);
        rowLayout->setSpacing(4);

        auto *header = new QHBoxLayout();
        auto *label = new QLabel(labelText, row);
        header->addWidget(label);
        header->addStretch();
        auto *preview = new QLabel(row);
        preview->setObjectName(QStringLiteral("scalePreview"));
        header->addWidget(preview);
        rowLayout->addLayout(header);

        auto *slider = new QSlider(Qt::Horizontal, row);
        slider->setRange(0, 2);
        slider->setTickPosition(QSlider::TicksBelow);
        slider->setTickInterval(1);
        slider->setPageStep(1);
        slider->setSingleStep(1);
        slider->setEnabled(enabled);
        const QString selected = JsonLoader::optionalString(group, QStringLiteral("selected"));
        slider->setValue(scaleToTick(selected.isEmpty() ? QStringLiteral("default") : selected));
        rowLayout->addWidget(slider);

        auto *ticks = new QHBoxLayout();
        ticks->addWidget(new QLabel(tr("Low"), row));
        ticks->addStretch();
        ticks->addWidget(new QLabel(tr("Recommended"), row));
        ticks->addStretch();
        ticks->addWidget(new QLabel(tr("High"), row));
        rowLayout->addLayout(ticks);

        if (!enabled) {
            auto *disabled = new QLabel(
                tr("Overflow swap is not recommended here (disk swap already present)."), row);
            disabled->setWordWrap(true);
            disabled->setObjectName(QStringLiteral("scaleHint"));
            rowLayout->addWidget(disabled);
        }

        *sliderOut = slider;
        *previewOut = preview;
        layout->addWidget(row);
        updatePreview(slider, preview, group, isZram);
        connect(slider, &QSlider::valueChanged, this, [this, slider, preview, group, isZram](int) {
            updatePreview(slider, preview, group, isZram);
            emit scalesChanged();
        });
    };

    const bool swapAvailable =
        JsonLoader::optionalBool(m_swapGroup, QStringLiteral("available"), false);
    addRow(tr("ZRAM size"), &m_zramSlider, &m_zramPreview, m_zramGroup, true, true);
    addRow(tr("Overflow swap file"), &m_swapSlider, &m_swapPreview, m_swapGroup, false,
           swapAvailable);
}

QString RecommendedDefaultsScalePanel::zramScale() const {
    return tickToScale(m_zramSlider ? m_zramSlider->value() : 1);
}

QString RecommendedDefaultsScalePanel::swapScale() const {
    if (!m_swapSlider || !m_swapSlider->isEnabled()) {
        return QStringLiteral("default");
    }
    return tickToScale(m_swapSlider->value());
}

void RecommendedDefaultsScalePanel::updatePreview(QSlider *slider, QLabel *preview,
                                                    const QJsonObject &group, bool isZram) {
    if (!slider || !preview) {
        return;
    }
    const QString key = tickToScale(slider->value());
    const QJsonObject opt = group.value(key).toObject();
    if (isZram) {
        const QString formula = JsonLoader::optionalString(opt, QStringLiteral("formula"));
        const quint64 approx = JsonLoader::optionalUInt64(opt, QStringLiteral("approx_mib"));
        if (!formula.isEmpty() && approx > 0) {
            preview->setText(tr("%1 (~%2 MiB)").arg(formula).arg(approx));
        } else if (!formula.isEmpty()) {
            preview->setText(formula);
        } else {
            preview->setText(tr("—"));
        }
    } else {
        const quint64 size = JsonLoader::optionalUInt64(opt, QStringLiteral("size_mib"));
        if (size > 0) {
            preview->setText(tr("%1 MiB").arg(size));
        } else {
            preview->setText(tr("—"));
        }
    }
}

int RecommendedDefaultsScalePanel::scaleToTick(const QString &selected) {
    if (selected == QLatin1String("low")) {
        return 0;
    }
    if (selected == QLatin1String("high")) {
        return 2;
    }
    return 1;
}

QString RecommendedDefaultsScalePanel::tickToScale(int tick) {
    if (tick <= 0) {
        return QStringLiteral("low");
    }
    if (tick >= 2) {
        return QStringLiteral("high");
    }
    return QStringLiteral("default");
}
