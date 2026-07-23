#include "dashboardwidget.h"

#include "formatutils.h"
#include "jsonloader.h"

#include <QBrush>
#include <QColor>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLabel>
#include <QProgressBar>
#include <QPushButton>
#include <QTableWidget>
#include <QVBoxLayout>

DashboardWidget::DashboardWidget(QWidget *parent) : QWidget(parent) {
    auto *layout = new QVBoxLayout(this);

    auto *memGroup = new QGroupBox(tr("Memory"), this);
    auto *memLayout = new QVBoxLayout(memGroup);
    m_memLabel = new QLabel(memGroup);
    m_memBar = new QProgressBar(memGroup);
    m_memBar->setRange(0, 100);
    memLayout->addWidget(m_memLabel);
    memLayout->addWidget(m_memBar);
    layout->addWidget(memGroup);

    auto *swapGroup = new QGroupBox(tr("Swap"), this);
    auto *swapLayout = new QVBoxLayout(swapGroup);
    m_swapLabel = new QLabel(swapGroup);
    m_swapBar = new QProgressBar(swapGroup);
    m_swapBar->setRange(0, 100);
    swapLayout->addWidget(m_swapLabel);
    swapLayout->addWidget(m_swapBar);
    layout->addWidget(swapGroup);

    auto *zramGroup = new QGroupBox(tr("ZRAM"), this);
    auto *zramLayout = new QVBoxLayout(zramGroup);
    m_zramCard = new QLabel(zramGroup);
    m_zramCard->setWordWrap(true);
    zramLayout->addWidget(m_zramCard);
    layout->addWidget(zramGroup);

    auto *healthRow = new QHBoxLayout();
    m_healthChip = new QLabel(this);
    m_healthChip->setAlignment(Qt::AlignCenter);
    m_healthChip->setMinimumHeight(28);
    healthRow->addWidget(m_healthChip);
    layout->addLayout(healthRow);

    m_recommendButton = new QPushButton(tr("Apply recommended defaults…"), this);
    m_recommendButton->setToolTip(
        tr("Review hardware-based ZRAM, sysctl, and overflow swap settings, then apply or stage them."));
    layout->addWidget(m_recommendButton);
    connect(m_recommendButton, &QPushButton::clicked, this,
            &DashboardWidget::recommendDefaultsRequested);

    m_detectStrip = new QLabel(this);
    m_detectStrip->setWordWrap(true);
    m_detectStrip->setStyleSheet(
        QStringLiteral("color: #495057; background: #e9ecef; border-radius: 4px; padding: 6px;"));
    layout->addWidget(m_detectStrip);

    auto *tableGroup = new QGroupBox(tr("Swap overview"), this);
    auto *tableLayout = new QVBoxLayout(tableGroup);
    m_swapTable = new QTableWidget(0, 6, tableGroup);
    m_swapTable->setHorizontalHeaderLabels(
        {tr("Device / path"), tr("Type"), tr("Status"), tr("Used / size"), tr("Priority"),
         tr("Source")});
    m_swapTable->horizontalHeader()->setStretchLastSection(true);
    m_swapTable->horizontalHeader()->setSectionResizeMode(0, QHeaderView::Stretch);
    m_swapTable->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_swapTable->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_swapTable->verticalHeader()->setVisible(false);
    tableLayout->addWidget(m_swapTable);
    layout->addWidget(tableGroup, 1);

    clearState();
}

void DashboardWidget::clearState() {
    m_memLabel->setText(tr("No data"));
    m_memBar->setValue(0);
    m_swapLabel->setText(tr("No data"));
    m_swapBar->setValue(0);
    m_zramCard->setText(tr("No active ZRAM devices"));
    m_swapTable->setRowCount(0);
    m_detectStrip->setText(tr("Detection: unavailable"));
    updateHealthChip(true, 0);
}

void DashboardWidget::updateHealthChip(bool healthy, int issueCount) {
    if (healthy) {
        m_healthChip->setText(tr("System healthy"));
        m_healthChip->setStyleSheet(
            QStringLiteral("background-color: #d4edda; color: #155724; border-radius: 4px; padding: 4px;"));
    } else if (issueCount > 0) {
        m_healthChip->setText(tr("%1 issue(s) found").arg(issueCount));
        m_healthChip->setStyleSheet(
            QStringLiteral("background-color: #fff3cd; color: #856404; border-radius: 4px; padding: 4px;"));
    } else {
        m_healthChip->setText(tr("Issues detected"));
        m_healthChip->setStyleSheet(
            QStringLiteral("background-color: #f8d7da; color: #721c24; border-radius: 4px; padding: 4px;"));
    }
}

void DashboardWidget::setStatusJson(const QString &json) {
    QString error;
    const QJsonObject root = JsonLoader::parseObject(json, &error);
    if (root.contains(QStringLiteral("error"))) {
        clearState();
        m_memLabel->setText(root.value(QStringLiteral("error")).toString());
        return;
    }

    const QJsonObject memory = root.value(QStringLiteral("memory")).toObject();
    const quint64 memTotalKb = JsonLoader::optionalUInt64(memory, QStringLiteral("mem_total_kb"));
    const quint64 memAvailKb = JsonLoader::optionalUInt64(memory, QStringLiteral("mem_available_kb"));
    const quint64 swapTotalKb = JsonLoader::optionalUInt64(memory, QStringLiteral("swap_total_kb"));
    const quint64 swapFreeKb = JsonLoader::optionalUInt64(memory, QStringLiteral("swap_free_kb"));

    const quint64 memUsedKb = memTotalKb > memAvailKb ? memTotalKb - memAvailKb : 0;
    const double memUsedRatio = memTotalKb > 0 ? static_cast<double>(memUsedKb) / memTotalKb : 0.0;
    m_memLabel->setText(tr("%1 total · %2 available · %3 used")
                            .arg(FormatUtils::formatBytes(memTotalKb * 1024),
                                 FormatUtils::formatBytes(memAvailKb * 1024),
                                 FormatUtils::formatPercent(memUsedRatio)));
    m_memBar->setValue(static_cast<int>(memUsedRatio * 100.0));

    const quint64 swapUsedKb = swapTotalKb > swapFreeKb ? swapTotalKb - swapFreeKb : 0;
    const double swapUsedRatio = swapTotalKb > 0 ? static_cast<double>(swapUsedKb) / swapTotalKb : 0.0;
    m_swapLabel->setText(tr("%1 total · %2 free · %3 used")
                             .arg(FormatUtils::formatBytes(swapTotalKb * 1024),
                                  FormatUtils::formatBytes(swapFreeKb * 1024),
                                  FormatUtils::formatPercent(swapUsedRatio)));
    m_swapBar->setValue(static_cast<int>(swapUsedRatio * 100.0));

    const QJsonArray zramDevices = root.value(QStringLiteral("zram_devices")).toArray();
    if (zramDevices.isEmpty()) {
        m_zramCard->setText(tr("No active ZRAM devices"));
    } else {
        QStringList lines;
        for (const QJsonValue &value : zramDevices) {
            const QJsonObject dev = value.toObject();
            const QString name = JsonLoader::optionalString(dev, QStringLiteral("name"));
            const QString algo = JsonLoader::optionalString(dev, QStringLiteral("algorithm"));
            const quint64 data = JsonLoader::optionalUInt64(dev, QStringLiteral("data_bytes"));
            const quint64 compr = JsonLoader::optionalUInt64(dev, QStringLiteral("compressed_bytes"));
            const quint64 disk = JsonLoader::optionalUInt64(dev, QStringLiteral("disk_size_bytes"));
            const quint64 streams = JsonLoader::optionalUInt64(dev, QStringLiteral("streams"));
            const QString mount = JsonLoader::optionalString(dev, QStringLiteral("mount_point"));

            lines << tr("<b>%1</b> · %2 · ratio %3 · disk %4 · %5 streams · mount %6")
                         .arg(name, algo, FormatUtils::compressionRatio(data, compr),
                              FormatUtils::formatBytes(disk), QString::number(streams),
                              mount.isEmpty() ? QStringLiteral("—") : mount);
        }
        m_zramCard->setText(lines.join(QStringLiteral("<br>")));
    }
}

void DashboardWidget::setSwapsJson(const QString &json) {
    QJsonParseError parseError;
    const QJsonDocument doc = QJsonDocument::fromJson(json.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError) {
        return;
    }

    if (doc.isObject() && doc.object().contains(QStringLiteral("error"))) {
        return;
    }

    const QJsonArray swaps = doc.isArray() ? doc.array() : QJsonArray();
    m_swapTable->setRowCount(swaps.size());
    int row = 0;
    for (const QJsonValue &value : swaps) {
        const QJsonObject entry = value.toObject();
        const QString name = JsonLoader::optionalString(entry, QStringLiteral("name"));
        const QString type = JsonLoader::optionalString(entry, QStringLiteral("swap_type"));
        const bool active = JsonLoader::optionalBool(entry, QStringLiteral("active"), true);
        const quint64 size = JsonLoader::optionalUInt64(entry, QStringLiteral("size_bytes"));
        const quint64 used = JsonLoader::optionalUInt64(entry, QStringLiteral("used_bytes"));
        const int priority = JsonLoader::optionalInt(entry, QStringLiteral("priority"));
        const QString source = JsonLoader::optionalString(entry, QStringLiteral("source"));

        const QString status = active ? tr("active") : tr("inactive");
        const QString sizeText =
            size > 0 ? QStringLiteral("%1 / %2").arg(FormatUtils::formatBytes(used),
                                                     FormatUtils::formatBytes(size))
                     : QStringLiteral("— / —");

        m_swapTable->setItem(row, 0, new QTableWidgetItem(name));
        m_swapTable->setItem(row, 1, new QTableWidgetItem(FormatUtils::humanizeEnum(type)));
        auto *statusItem = new QTableWidgetItem(status);
        if (!active) {
            statusItem->setForeground(QBrush(QColor(QStringLiteral("#856404"))));
        }
        m_swapTable->setItem(row, 2, statusItem);
        m_swapTable->setItem(row, 3, new QTableWidgetItem(sizeText));
        m_swapTable->setItem(row, 4, new QTableWidgetItem(QString::number(priority)));
        m_swapTable->setItem(row, 5, new QTableWidgetItem(FormatUtils::swapSourceLabel(source)));
        ++row;
    }
}

void DashboardWidget::setDoctorJson(const QString &json) {
    QString error;
    const QJsonObject root = JsonLoader::parseObject(json, &error);
    if (root.contains(QStringLiteral("error"))) {
        return;
    }
    const bool healthy = JsonLoader::optionalBool(root, QStringLiteral("healthy"), true);
    const QJsonArray issues = root.value(QStringLiteral("issues")).toArray();
    updateHealthChip(healthy, issues.size());
}

void DashboardWidget::setDetectionJson(const QString &json) {
    QString error;
    const QJsonObject root = JsonLoader::parseObject(json, &error);
    if (root.contains(QStringLiteral("error"))) {
        m_detectStrip->setText(tr("Detection: unavailable"));
        return;
    }
    QString distro = JsonLoader::optionalString(root, QStringLiteral("distro"));
    if (distro.isEmpty() && root.value(QStringLiteral("distro")).isObject()) {
        const QJsonObject distroObj = root.value(QStringLiteral("distro")).toObject();
        distro = JsonLoader::optionalString(distroObj, QStringLiteral("pretty_name"));
        if (distro.isEmpty()) {
            distro = JsonLoader::optionalString(distroObj, QStringLiteral("id"));
        }
    }
    const QString backend = JsonLoader::optionalString(root, QStringLiteral("zram_backend"));
    m_detectStrip->setText(
        tr("Detected: %1 · backend %2")
            .arg(distro.isEmpty() ? tr("unknown") : distro,
                 backend.isEmpty() ? tr("unknown") : FormatUtils::humanizeEnum(backend)));
}
