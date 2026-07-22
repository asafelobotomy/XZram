#ifndef SETTINGSWIDGET_H
#define SETTINGSWIDGET_H

#include <QWidget>

class QCheckBox;
class QComboBox;
class QLabel;
class QSpinBox;

class SettingsWidget : public QWidget {
    Q_OBJECT

public:
    explicit SettingsWidget(QWidget *parent = nullptr);

    int refreshIntervalMs() const;
    bool confirmBeforeApply() const;
    int pruneKeepDefault() const;

    void refreshStatus();

signals:
    void refreshIntervalChanged(int intervalMs);
    void confirmBeforeApplyChanged(bool enabled);
    void pruneKeepDefaultChanged(int keep);

private slots:
    void onIntervalChanged(int index);
    void onConfirmToggled(bool checked);
    void onPruneKeepChanged(int value);

private:
    void loadSettings();
    void saveSettings();

    QComboBox *m_intervalCombo;
    QCheckBox *m_confirmApplyCheck;
    QSpinBox *m_pruneKeepSpin;
    QLabel *m_cliPathLabel;
    QLabel *m_daemonStatusLabel;
    QLabel *m_versionLabel;
};

#endif
