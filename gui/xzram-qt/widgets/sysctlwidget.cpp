#include "sysctlwidget.h"

#include "jsonloader.h"
#include "xzramcli.h"

#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QJsonObject>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

namespace {
constexpr int kUnsetSentinel = -1;
}

SysctlWidget::SysctlWidget(QWidget *parent) : QWidget(parent) {
    auto *layout = new QVBoxLayout(this);

    auto *intro = new QLabel(
        tr("Tune VM reclaim behavior for zram-friendly swapping. Stage values here, then "
           "apply from the pending banner."),
        this);
    intro->setWordWrap(true);
    intro->setStyleSheet(QStringLiteral("color: #495057;"));
    layout->addWidget(intro);

    auto *group = new QGroupBox(tr("VM tuning"), this);
    auto *form = new QFormLayout(group);

    m_swappinessSpin = new QSpinBox(group);
    m_swappinessSpin->setRange(kUnsetSentinel, 200);
    m_swappinessSpin->setSpecialValueText(tr("not set"));
    m_swappinessSpin->setToolTip(
        tr("Higher values swap more aggressively. ZRAM setups often use 180."));
    form->addRow(tr("vm.swappiness"), m_swappinessSpin);

    m_boostSpin = new QSpinBox(group);
    m_boostSpin->setRange(kUnsetSentinel, 10000);
    m_boostSpin->setSpecialValueText(tr("not set"));
    m_boostSpin->setToolTip(tr("Boosts reclaim when memory is low."));
    form->addRow(tr("vm.watermark_boost_factor"), m_boostSpin);

    m_scaleSpin = new QSpinBox(group);
    m_scaleSpin->setRange(kUnsetSentinel, 10000);
    m_scaleSpin->setSpecialValueText(tr("not set"));
    m_scaleSpin->setToolTip(tr("Scales per-zone watermarks under memory pressure."));
    form->addRow(tr("vm.watermark_scale_factor"), m_scaleSpin);

    m_pageClusterSpin = new QSpinBox(group);
    m_pageClusterSpin->setRange(kUnsetSentinel, 8);
    m_pageClusterSpin->setSpecialValueText(tr("not set"));
    m_pageClusterSpin->setToolTip(tr("0 is recommended for zram (4K swap-in pages)."));
    form->addRow(tr("vm.page-cluster"), m_pageClusterSpin);

    layout->addWidget(group);

    auto *buttons = new QHBoxLayout();
    m_defaultsButton = new QPushButton(tr("Use recommended values"), this);
    m_stageButton = new QPushButton(tr("Stage changes"), this);
    m_defaultsButton->setToolTip(
        tr("Fill the form with the usual zram-friendly settings (does not apply them yet)."));
    m_stageButton->setToolTip(
        tr("Queue these memory-tuning values. They take effect only after you click Apply now in the banner."));
    buttons->addWidget(m_defaultsButton);
    buttons->addWidget(m_stageButton);
    buttons->addStretch();
    layout->addLayout(buttons);
    layout->addStretch();

    connect(m_defaultsButton, &QPushButton::clicked, this, &SysctlWidget::applyZramDefaults);
    connect(m_stageButton, &QPushButton::clicked, this, &SysctlWidget::stageChanges);
    connect(m_swappinessSpin, QOverload<int>::of(&QSpinBox::valueChanged), this,
            &SysctlWidget::updateActionEnabled);
    connect(m_boostSpin, QOverload<int>::of(&QSpinBox::valueChanged), this,
            &SysctlWidget::updateActionEnabled);
    connect(m_scaleSpin, QOverload<int>::of(&QSpinBox::valueChanged), this,
            &SysctlWidget::updateActionEnabled);
    connect(m_pageClusterSpin, QOverload<int>::of(&QSpinBox::valueChanged), this,
            &SysctlWidget::updateActionEnabled);

    captureBaseline();
    updateActionEnabled();
}

void SysctlWidget::setSpinValue(QSpinBox *spin, const QJsonObject &obj, const QString &key) {
    const QJsonValue value = obj.value(key);
    if (value.isNull() || value.isUndefined()) {
        spin->setValue(kUnsetSentinel);
    } else {
        spin->setValue(value.toInt());
    }
}

void SysctlWidget::setSysctlJson(const QString &json) {
    QString error;
    const QJsonObject root = JsonLoader::parseObject(json, &error);
    if (root.contains(QStringLiteral("error"))) {
        return;
    }
    setSpinValue(m_swappinessSpin, root, QStringLiteral("swappiness"));
    setSpinValue(m_boostSpin, root, QStringLiteral("watermark_boost_factor"));
    setSpinValue(m_scaleSpin, root, QStringLiteral("watermark_scale_factor"));
    setSpinValue(m_pageClusterSpin, root, QStringLiteral("page_cluster"));
    captureBaseline();
    updateActionEnabled();
}

void SysctlWidget::captureBaseline() {
    m_baselineSwappiness = m_swappinessSpin->value();
    m_baselineBoost = m_boostSpin->value();
    m_baselineScale = m_scaleSpin->value();
    m_baselinePageCluster = m_pageClusterSpin->value();
}

bool SysctlWidget::formDirty() const {
    return m_swappinessSpin->value() != m_baselineSwappiness
        || m_boostSpin->value() != m_baselineBoost
        || m_scaleSpin->value() != m_baselineScale
        || m_pageClusterSpin->value() != m_baselinePageCluster;
}

void SysctlWidget::updateActionEnabled() {
    const bool dirty = formDirty();
    m_stageButton->setEnabled(dirty);
    const bool alreadyDefaults = m_swappinessSpin->value() == 180 && m_boostSpin->value() == 0
        && m_scaleSpin->value() == 125 && m_pageClusterSpin->value() == 0;
    m_defaultsButton->setEnabled(!alreadyDefaults);
}

void SysctlWidget::applyZramDefaults() {
    m_swappinessSpin->setValue(180);
    m_boostSpin->setValue(0);
    m_scaleSpin->setValue(125);
    m_pageClusterSpin->setValue(0);
    updateActionEnabled();
}

void SysctlWidget::stageChanges() {
    if (!formDirty()) {
        return;
    }

    QStringList flags;
    if (m_swappinessSpin->value() != kUnsetSentinel) {
        flags << QStringLiteral("--swappiness") << QString::number(m_swappinessSpin->value());
    }
    if (m_boostSpin->value() != kUnsetSentinel) {
        flags << QStringLiteral("--watermark-boost-factor")
              << QString::number(m_boostSpin->value());
    }
    if (m_scaleSpin->value() != kUnsetSentinel) {
        flags << QStringLiteral("--watermark-scale-factor")
              << QString::number(m_scaleSpin->value());
    }
    if (m_pageClusterSpin->value() != kUnsetSentinel) {
        flags << QStringLiteral("--page-cluster") << QString::number(m_pageClusterSpin->value());
    }

    if (flags.isEmpty()) {
        QMessageBox::information(this, tr("Stage"), tr("Set at least one sysctl value."));
        return;
    }

    QString error;
    if (!XzramCli::sysctlSet(flags, &error)) {
        QMessageBox::warning(this, tr("Stage failed"), error);
        return;
    }
    captureBaseline();
    updateActionEnabled();
    emit stagingChanged();
}
