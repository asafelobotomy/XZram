#include "settingswidget.h"

#include "xzramcli.h"

#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QFormLayout>
#include <QGroupBox>
#include <QLabel>
#include <QSettings>
#include <QSpinBox>
#include <QVBoxLayout>

namespace {
constexpr auto kSettingsOrg = "XZram";
constexpr auto kSettingsApp = "xzram-qt";
constexpr auto kKeyRefreshMs = "refreshIntervalMs";
constexpr auto kKeyConfirmApply = "confirmBeforeApply";
constexpr auto kKeyPruneKeep = "pruneKeepDefault";
} // namespace

SettingsWidget::SettingsWidget(QWidget *parent) : QWidget(parent) {
    auto *layout = new QVBoxLayout(this);

    auto *intro = new QLabel(
        tr("Preferences for the XZram GUI. Native reads and changes use the xzram CLI; "
           "xzramd is optional for other clients."),
        this);
    intro->setWordWrap(true);
    intro->setStyleSheet(QStringLiteral("color: #495057;"));
    layout->addWidget(intro);

    auto *prefs = new QGroupBox(tr("Preferences"), this);
    auto *form = new QFormLayout(prefs);

    m_intervalCombo = new QComboBox(prefs);
    m_intervalCombo->addItem(tr("Off"), 0);
    m_intervalCombo->addItem(tr("2 seconds"), 2000);
    m_intervalCombo->addItem(tr("5 seconds"), 5000);
    m_intervalCombo->addItem(tr("10 seconds"), 10000);
    form->addRow(tr("Auto-refresh"), m_intervalCombo);

    m_confirmApplyCheck = new QCheckBox(tr("Ask before applying pending changes"), prefs);
    form->addRow(tr("Safety"), m_confirmApplyCheck);

    m_pruneKeepSpin = new QSpinBox(prefs);
    m_pruneKeepSpin->setRange(1, 1000);
    m_pruneKeepSpin->setValue(50);
    form->addRow(tr("Default prune keep"), m_pruneKeepSpin);

    layout->addWidget(prefs);

    auto *status = new QGroupBox(tr("Status"), this);
    auto *statusForm = new QFormLayout(status);
    m_cliPathLabel = new QLabel(status);
    m_cliPathLabel->setTextInteractionFlags(Qt::TextSelectableByMouse);
    m_cliPathLabel->setWordWrap(true);
    statusForm->addRow(tr("xzram CLI"), m_cliPathLabel);

    m_daemonStatusLabel = new QLabel(status);
    statusForm->addRow(tr("xzramd"), m_daemonStatusLabel);

    m_versionLabel = new QLabel(status);
    m_versionLabel->setTextInteractionFlags(Qt::TextSelectableByMouse);
    statusForm->addRow(tr("Version"), m_versionLabel);
    layout->addWidget(status);
    layout->addStretch();

    loadSettings();
    refreshStatus();

    connect(m_intervalCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &SettingsWidget::onIntervalChanged);
    connect(m_confirmApplyCheck, &QCheckBox::toggled, this, &SettingsWidget::onConfirmToggled);
    connect(m_pruneKeepSpin, QOverload<int>::of(&QSpinBox::valueChanged), this,
            &SettingsWidget::onPruneKeepChanged);
}

void SettingsWidget::loadSettings() {
    QSettings settings(QString::fromUtf8(kSettingsOrg), QString::fromUtf8(kSettingsApp));
    const int ms = settings.value(QString::fromUtf8(kKeyRefreshMs), 5000).toInt();
    int index = m_intervalCombo->findData(ms);
    if (index < 0) {
        index = m_intervalCombo->findData(5000);
    }
    m_intervalCombo->setCurrentIndex(index >= 0 ? index : 2);
    m_confirmApplyCheck->setChecked(
        settings.value(QString::fromUtf8(kKeyConfirmApply), true).toBool());
    m_pruneKeepSpin->setValue(settings.value(QString::fromUtf8(kKeyPruneKeep), 50).toInt());
}

void SettingsWidget::saveSettings() {
    QSettings settings(QString::fromUtf8(kSettingsOrg), QString::fromUtf8(kSettingsApp));
    settings.setValue(QString::fromUtf8(kKeyRefreshMs), refreshIntervalMs());
    settings.setValue(QString::fromUtf8(kKeyConfirmApply), confirmBeforeApply());
    settings.setValue(QString::fromUtf8(kKeyPruneKeep), pruneKeepDefault());
}

int SettingsWidget::refreshIntervalMs() const {
    return m_intervalCombo->currentData().toInt();
}

bool SettingsWidget::confirmBeforeApply() const {
    return m_confirmApplyCheck->isChecked();
}

int SettingsWidget::pruneKeepDefault() const {
    return m_pruneKeepSpin->value();
}

void SettingsWidget::refreshStatus() {
    m_cliPathLabel->setText(XzramCli::findBinary());
    m_daemonStatusLabel->setText(XzramCli::daemonIsActive() ? tr("running (optional)")
                                                            : tr("not running (optional)"));
    const QString guiVersion = QApplication::applicationVersion();
    m_versionLabel->setText(guiVersion.isEmpty() ? tr("unknown") : guiVersion);
    m_versionLabel->setToolTip(
        tr("xzram-qt application version (should match the xzram CLI package version)."));
}

void SettingsWidget::onIntervalChanged(int) {
    saveSettings();
    emit refreshIntervalChanged(refreshIntervalMs());
}

void SettingsWidget::onConfirmToggled(bool checked) {
    saveSettings();
    emit confirmBeforeApplyChanged(checked);
}

void SettingsWidget::onPruneKeepChanged(int value) {
    saveSettings();
    emit pruneKeepDefaultChanged(value);
}
