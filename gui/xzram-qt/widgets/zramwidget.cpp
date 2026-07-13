#include "zramwidget.h"

#include "dbusclient.h"
#include "formatutils.h"
#include "jsonloader.h"

#include <QComboBox>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

ZramWidget::ZramWidget(DbusClient *client, QWidget *parent)
    : QWidget(parent), m_client(client) {
    auto *layout = new QVBoxLayout(this);

    auto *statsGroup = new QGroupBox(tr("Live device stats"), this);
    auto *statsLayout = new QVBoxLayout(statsGroup);
    m_statsLabel = new QLabel(statsGroup);
    m_statsLabel->setWordWrap(true);
    statsLayout->addWidget(m_statsLabel);
    layout->addWidget(statsGroup);

    m_mismatchWarning = new QLabel(this);
    m_mismatchWarning->setWordWrap(true);
    m_mismatchWarning->setStyleSheet(
        QStringLiteral("color: #856404; background: #fff3cd; padding: 8px; border-radius: 4px;"));
    m_mismatchWarning->hide();
    layout->addWidget(m_mismatchWarning);

    auto *configGroup = new QGroupBox(tr("Configuration"), this);
    auto *form = new QFormLayout(configGroup);

    m_deviceEdit = new QLineEdit(configGroup);
    m_deviceEdit->setReadOnly(true);
    form->addRow(tr("Device"), m_deviceEdit);

    m_sizeEdit = new QLineEdit(configGroup);
    m_sizeEdit->setToolTip(
        tr("systemd-zram-generator size expression, e.g. min(ram / 2, 4096)"));
    form->addRow(tr("Size formula"), m_sizeEdit);

    m_residentLimitEdit = new QLineEdit(configGroup);
    m_residentLimitEdit->setPlaceholderText(tr("optional, e.g. ram / 2"));
    m_residentLimitEdit->setToolTip(
        tr("Caps RAM used for compressed pages (zram-resident-limit). Leave empty for no limit."));
    form->addRow(tr("Resident limit"), m_residentLimitEdit);

    m_algoCombo = new QComboBox(configGroup);
    m_algoCombo->addItems({QStringLiteral("zstd"), QStringLiteral("lz4"), QStringLiteral("lzo"),
                           QStringLiteral("lzo-rle"), QStringLiteral("deflate"),
                           QStringLiteral("842"), QStringLiteral("lz4hc")});
    form->addRow(tr("Algorithm"), m_algoCombo);

    m_prioritySpin = new QSpinBox(configGroup);
    m_prioritySpin->setRange(0, 32767);
    m_prioritySpin->setValue(100);
    form->addRow(tr("Swap priority"), m_prioritySpin);

    layout->addWidget(configGroup);

    auto *buttons = new QHBoxLayout();
    m_stageButton = new QPushButton(tr("Stage changes"), this);
    m_disableButton = new QPushButton(tr("Disable ZRAM"), this);
    m_migrateButton = new QPushButton(tr("Migrate from zram-tools"), this);
    buttons->addWidget(m_stageButton);
    buttons->addWidget(m_disableButton);
    buttons->addWidget(m_migrateButton);
    buttons->addStretch();
    layout->addLayout(buttons);
    layout->addStretch();

    connect(m_stageButton, &QPushButton::clicked, this, &ZramWidget::stageChanges);
    connect(m_disableButton, &QPushButton::clicked, this, &ZramWidget::disableZram);
    connect(m_migrateButton, &QPushButton::clicked, this, &ZramWidget::migrateZram);

    setEditingEnabled(false);
    m_statsLabel->setText(tr("No ZRAM data"));
}

void ZramWidget::setDaemonAvailable(bool available) {
    m_daemonAvailable = available;
    setEditingEnabled(available);
}

void ZramWidget::setEditingEnabled(bool enabled) {
    m_sizeEdit->setEnabled(enabled);
    m_residentLimitEdit->setEnabled(enabled);
    m_algoCombo->setEnabled(enabled);
    m_prioritySpin->setEnabled(enabled);
    m_stageButton->setEnabled(enabled);
    m_disableButton->setEnabled(enabled);
    m_migrateButton->setEnabled(enabled);
}

void ZramWidget::setStatusJson(const QString &json) {
    QString error;
    const QJsonObject root = JsonLoader::parseObject(json, &error);
    if (root.contains(QStringLiteral("error"))) {
        m_statsLabel->setText(root.value(QStringLiteral("error")).toString());
        return;
    }
    updateLiveStats(root);
    updateMismatchWarning();
}

void ZramWidget::updateLiveStats(const QJsonObject &status) {
    const QJsonArray devices = status.value(QStringLiteral("zram_devices")).toArray();
    if (devices.isEmpty()) {
        m_statsLabel->setText(tr("No active ZRAM devices"));
        m_activeAlgorithm.clear();
        return;
    }

    QStringList lines;
    for (const QJsonValue &value : devices) {
        const QJsonObject dev = value.toObject();
        m_activeAlgorithm = JsonLoader::optionalString(dev, QStringLiteral("algorithm"));
        const QString name = JsonLoader::optionalString(dev, QStringLiteral("name"));
        const quint64 data = JsonLoader::optionalUInt64(dev, QStringLiteral("data_bytes"));
        const quint64 compr = JsonLoader::optionalUInt64(dev, QStringLiteral("compressed_bytes"));
        const quint64 streams = JsonLoader::optionalUInt64(dev, QStringLiteral("streams"));
        const QString mount = JsonLoader::optionalString(dev, QStringLiteral("mount_point"));

        lines << tr("%1: active algorithm <b>%2</b>, compression %3, %4 streams, mount %5")
                     .arg(name, m_activeAlgorithm, FormatUtils::compressionRatio(data, compr),
                          QString::number(streams),
                          mount.isEmpty() ? QStringLiteral("—") : mount);
    }
    m_statsLabel->setText(lines.join(QStringLiteral("<br>")));
}

void ZramWidget::setZramConfigJson(const QString &json) {
    QJsonParseError parseError;
    const QJsonDocument doc = QJsonDocument::fromJson(json.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError) {
        return;
    }
    if (doc.isNull()) {
        updateConfigForm(QJsonValue::Null);
    } else {
        updateConfigForm(doc.isObject() ? QJsonValue(doc.object()) : QJsonValue(doc.array()));
    }
    updateMismatchWarning();
}

void ZramWidget::updateConfigForm(const QJsonValue &config) {
    if (config.isNull()) {
        m_deviceEdit->setText(QStringLiteral("zram0"));
        m_sizeEdit->setText(QStringLiteral("min(ram / 2, 4096)"));
        m_residentLimitEdit->clear();
        m_algoCombo->setCurrentText(QStringLiteral("zstd"));
        m_prioritySpin->setValue(100);
        return;
    }

    const QJsonObject obj = config.toObject();
    if (obj.contains(QStringLiteral("error"))) {
        return;
    }

    m_deviceEdit->setText(JsonLoader::optionalString(obj, QStringLiteral("device")));
    const QString size = JsonLoader::optionalString(obj, QStringLiteral("zram_size"));
    if (!size.isEmpty()) {
        m_sizeEdit->setText(size);
    }
    const QString resident =
        JsonLoader::optionalString(obj, QStringLiteral("zram_resident_limit"));
    m_residentLimitEdit->setText(resident);
    const QString algo = JsonLoader::optionalString(obj, QStringLiteral("compression_algorithm"));
    if (!algo.isEmpty()) {
        const int idx = m_algoCombo->findText(algo);
        if (idx >= 0) {
            m_algoCombo->setCurrentIndex(idx);
        } else {
            m_algoCombo->addItem(algo);
            m_algoCombo->setCurrentText(algo);
        }
    }
    if (obj.contains(QStringLiteral("swap_priority"))) {
        m_prioritySpin->setValue(JsonLoader::optionalInt(obj, QStringLiteral("swap_priority"), 100));
    }
}

void ZramWidget::setDetectionJson(const QString &json) {
    QString error;
    const QJsonObject root = JsonLoader::parseObject(json, &error);
    const QString backend = JsonLoader::optionalString(root, QStringLiteral("zram_backend"));
    m_migrateButton->setVisible(backend == QLatin1String("zram_tools"));
}

void ZramWidget::updateMismatchWarning() {
    const QString configured = m_algoCombo->currentText();
    if (!m_activeAlgorithm.isEmpty() && !configured.isEmpty() &&
        configured != m_activeAlgorithm) {
        m_mismatchWarning->setText(
            tr("Configured algorithm is <b>%1</b> but the running device uses <b>%2</b>. "
               "Apply staged changes or restart the zram unit to align them.")
                .arg(configured, m_activeAlgorithm));
        m_mismatchWarning->show();
    } else {
        m_mismatchWarning->hide();
    }
}

void ZramWidget::stageChanges() {
    if (!m_daemonAvailable) {
        return;
    }

    QJsonObject config;
    config.insert(QStringLiteral("device"), m_deviceEdit->text());
    config.insert(QStringLiteral("zram_size"), m_sizeEdit->text());
    const QString resident = m_residentLimitEdit->text().trimmed();
    if (!resident.isEmpty()) {
        config.insert(QStringLiteral("zram_resident_limit"), resident);
    }
    config.insert(QStringLiteral("compression_algorithm"), m_algoCombo->currentText());
    config.insert(QStringLiteral("swap_priority"), m_prioritySpin->value());

    QString error;
    if (!m_client->configureZram(QString::fromUtf8(QJsonDocument(config).toJson(QJsonDocument::Compact)),
                                 &error)) {
        QMessageBox::warning(this, tr("Stage failed"), error);
        return;
    }
    emit stagingChanged();
}

void ZramWidget::disableZram() {
    if (!m_daemonAvailable) {
        return;
    }
    const auto answer = QMessageBox::question(
        this, tr("Disable ZRAM"),
        tr("Disable ZRAM swap? This takes effect immediately when using the daemon."));
    if (answer != QMessageBox::Yes) {
        return;
    }

    QString error;
    if (!m_client->disableZram(&error)) {
        QMessageBox::warning(this, tr("Disable failed"), error);
        return;
    }
    emit stagingChanged();
}

void ZramWidget::migrateZram() {
    if (!m_daemonAvailable) {
        return;
    }
    QString error;
    if (!m_client->migrateZram(&error)) {
        QMessageBox::warning(this, tr("Migrate failed"), error);
        return;
    }
    QMessageBox::information(this, tr("Migrate"),
                           tr("Migration from zram-tools staged. Use Apply to activate."));
    emit stagingChanged();
}
