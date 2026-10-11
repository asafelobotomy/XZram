#ifndef XZRAMCLI_H
#define XZRAMCLI_H

#include <QDateTime>
#include <QMetaType>
#include <QString>
#include <QStringList>
#include <QtGlobal>

namespace XzramCli {

/// CLI exit codes (see `xzram --help`).
constexpr int kExitOk = 0;
constexpr int kExitError = 1;
constexpr int kExitUsage = 2;
constexpr int kExitDoctorUnhealthy = 3;

struct RunResult {
    bool ok = false;
    int exitCode = -1;
    QString stdoutText;
    QString stderrText;
    QString error;
};

/// Path to the xzram CLI. Release builds use only the install-time path; debug overrides
/// (`XZRAM_CLI` + `XZRAM_ALLOW_DEV_CLI`) exist only when built with XZRAM_DEV_OVERRIDES=ON.
QString findBinary();

/// Build a RunResult from process output (shared by sync run and CliJob).
/// `startedAt` gates use of /var/lib/xzram/last_error (only if written after the start).
/// `extraOkExitCode` (>= 0) is an additional exit code treated as success.
RunResult resultFromOutput(int exitCode, bool crashed, const QString &stdoutText,
                           const QString &stderrText, const QDateTime &startedAt,
                           int extraOkExitCode = -1);

/// Synchronous run for quick, unprivileged reads only (kills the CLI on timeout).
/// Privileged operations must go through runPrivilegedCli (privilegedjob.h).
RunResult run(const QStringList &args, int timeoutMs = 30000, int extraOkExitCode = -1);

/// Convenience: stdout on success, or {"error":"..."} JSON string on failure.
QString runJson(const QStringList &args, int timeoutMs = 30000, int extraOkExitCode = -1);

// --- Argv builders (xzramcli_args.cpp) ---
QStringList argsApply();
QStringList argsClearPending();
QStringList argsDefaultsApply(const QString &zramScale, const QString &swapScale);
QStringList argsDefaultsStage(const QString &zramScale, const QString &swapScale);
QStringList argsDefaultsRecommend(const QString &zramScale, const QString &swapScale);
QStringList argsDefaultsOptimizeLinked(const QString &anchor);
QStringList argsZramSet(const QString &device, const QString &size, const QString &algorithm,
                        int priority);
QStringList argsZramDisable();
QStringList argsZramMigrate();
QStringList argsSwapfileCreate(const QString &path, quint64 sizeMb, int priority, bool prepare,
                               bool mkdirParents);
QStringList argsSwapfileResize(const QString &path, quint64 sizeMb);
QStringList argsSwapfileRemove(const QString &path);
QStringList argsSwapfilePrepare(const QString &path, bool mkdirParents);
QStringList argsSysctlSet(const QStringList &flagArgs);
QStringList argsSwapOn(const QString &device);
QStringList argsSwapOff(const QString &device);
QStringList argsSnapshotCreate(const QString &label);
QStringList argsSnapshotRestore(const QString &id);
QStringList argsSnapshotDelete(const QString &id);
QStringList argsSnapshotPrune(int keep);
QStringList argsRollback();

// --- Read-only argv (used by the async refresher) ---
QStringList argsStatusJson();
QStringList argsDetectJson();
QStringList argsDoctorJson();
QStringList argsZramShowJson();
QStringList argsSwapfileListJson();
QStringList argsSwapListJson();
QStringList argsSysctlShowJson();
QStringList argsPendingShowJson();
QStringList argsSnapshotListJson();

// --- Synchronous JSON reads (unprivileged, quick) ---
QString swapfileCheckJson(const QString &path);
QString pendingJson();
QString recommendedDefaultsJson();
QString recommendedDefaultsJson(const QString &zramScale, const QString &swapScale);
QString snapshotsJson();

} // namespace XzramCli

Q_DECLARE_METATYPE(XzramCli::RunResult)

#endif
