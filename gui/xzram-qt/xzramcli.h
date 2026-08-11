#ifndef XZRAMCLI_H
#define XZRAMCLI_H

#include <QString>
#include <QStringList>
#include <QtGlobal>
#include <QMetaType>

namespace XzramCli {

struct RunResult {
    bool ok = false;
    int exitCode = -1;
    QString stdoutText;
    QString stderrText;
    QString error;
};

QString findBinary();

/// Build a RunResult from process output (shared by sync run and CliJob).
RunResult resultFromOutput(int exitCode, bool crashed, const QString &stdoutText,
                           const QString &stderrText);

/// Run xzram; on failure prefers process streams then /var/lib/xzram/last_error.
RunResult run(const QStringList &args, int timeoutMs = 120000);

/// Convenience: stdout on success, or {"error":"..."} JSON string on failure.
QString runJson(const QStringList &args, int timeoutMs = 30000);

bool runOk(const QStringList &args, QString *error = nullptr, int timeoutMs = 120000);

// --- Argv builders for long-running async jobs ---
QStringList argsApply();
QStringList argsDefaultsApply();
QStringList argsDefaultsApply(const QString &zramScale, const QString &swapScale);
QStringList argsDefaultsStage(const QString &zramScale, const QString &swapScale);
QStringList argsDefaultsRecommend(const QString &zramScale, const QString &swapScale);
QStringList argsDefaultsOptimizeLinked(const QString &anchor);
QStringList argsSwapfileCreate(const QString &path, quint64 sizeMb, int priority);
QStringList argsSwapfileCreate(const QString &path, quint64 sizeMb, int priority, bool prepare,
                               bool mkdirParents);
QStringList argsSwapfileResize(const QString &path, quint64 sizeMb);
QStringList argsSnapshotRestore(const QString &id);
QStringList argsRollback();

// --- JSON reads ---
QString statusJson();
QString detectionJson();
QString doctorJson();
QString zramConfigJson();
QString swapfilesJson();
QString swapfileCheckJson(const QString &path);
QString swapsJson();
QString sysctlJson();
QString pendingJson();
QString recommendedDefaultsJson();
QString recommendedDefaultsJson(const QString &zramScale, const QString &swapScale);
QString snapshotsJson();

// --- Mutations (no --now unless noted) ---
bool apply(QString *error = nullptr);
bool clearPending(QString *error = nullptr);
bool daemonStart(QString *error = nullptr);
bool daemonIsActive();
bool defaultsStage(QString *error = nullptr);
bool defaultsStage(const QString &zramScale, const QString &swapScale, QString *error = nullptr);
bool defaultsApply(QString *error = nullptr);
bool defaultsApply(const QString &zramScale, const QString &swapScale, QString *error = nullptr);

bool zramSet(const QString &device, const QString &size, const QString &algorithm, int priority,
             QString *error = nullptr);
bool zramDisable(QString *error = nullptr);
bool zramMigrate(QString *error = nullptr);

bool swapfileCreate(const QString &path, quint64 sizeMb, int priority, QString *error = nullptr);
bool swapfileResize(const QString &path, quint64 sizeMb, QString *error = nullptr);
bool swapfileRemove(const QString &path, QString *error = nullptr);
bool swapfilePrepare(const QString &path, bool mkdirParents, QString *error = nullptr);

bool sysctlSet(const QStringList &flagArgs, QString *error = nullptr);

bool swapOn(const QString &device, QString *error = nullptr);
bool swapOff(const QString &device, QString *error = nullptr);

bool snapshotCreate(const QString &label, QString *error = nullptr);
bool snapshotRestore(const QString &id, QString *error = nullptr);
bool snapshotDelete(const QString &id, QString *error = nullptr);
bool snapshotPrune(int keep, QString *error = nullptr);
bool rollback(QString *error = nullptr);

} // namespace XzramCli

Q_DECLARE_METATYPE(XzramCli::RunResult)

#endif
