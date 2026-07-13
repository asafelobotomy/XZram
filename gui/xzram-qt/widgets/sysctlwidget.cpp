#include "sysctlwidget.h"

#include "dbusclient.h"
#include "jsonloader.h"

#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

namespace {
constexpr int kUnsetSentinel = -1;
}

SysctlWidget::SysctlWidget(DbusClient *client, QWidget *parent)
    : QWidget(parent), m_client(client) {
    auto *layout = new QVBoxLayout(this);

    m_unavailableLabel = new QLabel(
        tr("Sysctl changes require xzramd. Start the service or use: xzram sysctl set"), this);
    m_unavailableLabel->setWordWrap(true);
    m_unavailableLabel->hide();
    layout->addWidget(m_unavailableLabel);

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
    m_defaultsButton = new QPushButton(tr("Apply zram tuning defaults"), this);
    m_stageButton = new QPushButton(tr("Stage changes"), this);
    buttons->addWidget(m_defaultsButton);
    buttons->addWidget(m_stageButton);
    buttons->addStretch();
    layout->addLayout(buttons);
    layout->addStretch();

    connect(m_defaultsButton, &QPushButton::clicked, this, &SysctlWidget::applyZramDefaults);
    connect(m_stageButton, &QPushButton::clicked, this, &SysctlWidget::stageChanges);

    setEditingEnabled(false);
}

void SysctlWidget::setDaemonAvailable(bool available) {
    m_daemonAvailable = available;
    m_unavailableLabel->setVisible(!available);
    setEditingEnabled(available);
}

void SysctlWidget::setEditingEnabled(bool enabled) {
    m_swappinessSpin->setEnabled(enabled);
    m_boostSpin->setEnabled(enabled);
    m_scaleSpin->setEnabled(enabled);
    m_pageClusterSpin->setEnabled(enabled);
    m_defaultsButton->setEnabled(enabled);
    m_stageButton->setEnabled(enabled);
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
}

void SysctlWidget::applyZramDefaults() {
    m_swappinessSpin->setValue(180);
    m_boostSpin->setValue(0);
    m_scaleSpin->setValue(125);
    m_pageClusterSpin->setValue(0);
}

void SysctlWidget::stageChanges() {
    if (!m_daemonAvailable) {
        return;
    }

    QJsonObject values;
    if (m_swappinessSpin->value() != kUnsetSentinel) {
        values.insert(QStringLiteral("swappiness"), m_swappinessSpin->value());
    }
    if (m_boostSpin->value() != kUnsetSentinel) {
        values.insert(QStringLiteral("watermark_boost_factor"), m_boostSpin->value());
    }
    if (m_scaleSpin->value() != kUnsetSentinel) {
        values.insert(QStringLiteral("watermark_scale_factor"), m_scaleSpin->value());
    }
    if (m_pageClusterSpin->value() != kUnsetSentinel) {
        values.insert(QStringLiteral("page_cluster"), m_pageClusterSpin->value());
    }

    if (values.isEmpty()) {
        QMessageBox::information(this, tr("Stage"), tr("Set at least one sysctl value."));
        return;
    }

    QString error;
    if (!m_client->setSysctl(
            QString::fromUtf8(QJsonDocument(values).toJson(QJsonDocument::Compact)), &error)) {
        QMessageBox::warning(this, tr("Stage failed"), error);
        return;
    }
    emit stagingChanged();
}
