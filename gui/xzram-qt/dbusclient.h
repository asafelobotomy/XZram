#ifndef DBUSCLIENT_H
#define DBUSCLIENT_H

#include <QString>

class DbusClient {
public:
    static constexpr auto kBusName = "io.github.XZram1";

    bool isRegistered() const;
    bool ensureAvailable(int timeoutMs = 5000, QString *error = nullptr) const;
    bool startService(QString *error = nullptr) const;
    bool startServiceViaHelper(QString *error = nullptr) const;

    QString getStatusJson() const;
    QString getDetectionJson() const;
    QString getDoctorJson() const;
    QString getZramConfigJson() const;
    QString listSwapfilesJson() const;
    QString listSwapsJson() const;
    QString getSysctlJson() const;
    QString getPendingJson() const;

    QString getRecommendedDefaultsJson() const;
    bool stageRecommendedDefaults(QString *error = nullptr) const;

    bool configureZram(const QString &configJson, QString *error = nullptr) const;
    bool disableZram(QString *error = nullptr) const;
    bool createSwapfile(const QString &path, quint64 sizeMb, int priority,
                        QString *error = nullptr) const;
    bool removeSwapfile(const QString &path, QString *error = nullptr) const;
    bool resizeSwapfile(const QString &path, quint64 sizeMb, QString *error = nullptr) const;
    bool setSysctl(const QString &valuesJson, QString *error = nullptr) const;
    bool migrateZram(QString *error = nullptr) const;
    bool clearPending(QString *error = nullptr) const;
    bool applyPending(QString *error = nullptr) const;

    QString checkSwapfileBtrfsJson(const QString &path) const;
    bool prepareSwapfileBtrfs(const QString &path, bool mkdirParents, QString *error = nullptr) const;
    bool prepareSwapfileBtrfsViaHelper(const QString &path, bool mkdirParents,
                                       QString *error = nullptr) const;

    QString listSnapshotsJson() const;
    bool createSnapshot(const QString &trigger, const QString &label = QString(),
                        QString *error = nullptr) const;
    bool restoreSnapshot(const QString &id, QString *error = nullptr) const;

private:
    QString callJsonMethod(const char *method) const;
    QString callJsonMethodWithArgs(const char *method, const QVariantList &args) const;
    bool callVoidMethod(const char *method, const QVariantList &args, QString *error) const;
};

#endif
