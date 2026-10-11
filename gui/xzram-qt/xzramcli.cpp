#include "xzramcli.h"

#include <QFile>
#include <QFileInfo>
#include <QIODevice>
#include <QJsonDocument>
#include <QJsonObject>
#include <QObject>
#include <QProcess>

#ifndef XZRAM_CLI_INSTALL_PATH
#define XZRAM_CLI_INSTALL_PATH "/usr/bin/xzram"
#endif

namespace {

constexpr auto kLastErrorPath = "/var/lib/xzram/last_error";

/// pkexec: authentication dialog dismissed / not authorized.
constexpr int kPkexecDismissed = 126;
constexpr int kPkexecNotAuthorized = 127;

/// last_error contents, only when the file was written at/after `since` (else stale).
QString readFreshLastError(const QDateTime &since) {
    const QFileInfo info(QString::fromUtf8(kLastErrorPath));
    if (!since.isValid() || !info.exists() || info.lastModified() < since) {
        return {};
    }
    QFile file(info.filePath());
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) {
        return {};
    }
    return QString::fromUtf8(file.readAll()).trimmed();
}

bool isSystemdRunNoise(const QString &text) {
    return text.contains(QStringLiteral("Running as unit:"))
        || text.contains(QStringLiteral("Finished with result:"))
        || text.contains(QStringLiteral("Main processes terminated with:"))
        || text.contains(QStringLiteral("Service runtime:"));
}

QString bestError(int exitCode, const QString &stderrText, const QString &stdoutText,
                  const QDateTime &startedAt) {
    // Authorization failures: the helper never ran, so last_error can only be stale.
    if (exitCode == kPkexecDismissed) {
        return QObject::tr("Authorization cancelled");
    }
    if (exitCode == kPkexecNotAuthorized) {
        return QObject::tr("Authorization denied");
    }
    // Prefer this-process streams over shared last_error (avoids stale/cross-process TOCTOU).
    if ((stderrText.contains(QStringLiteral("xzram-helper:"))
         || stderrText.contains(QStringLiteral("xzram:")))
        && !isSystemdRunNoise(stderrText)) {
        return stderrText;
    }
    if (stdoutText.contains(QStringLiteral("xzram-helper:"))
        && !isSystemdRunNoise(stdoutText)) {
        return stdoutText;
    }
    if (!stderrText.isEmpty() && !isSystemdRunNoise(stderrText)) {
        return stderrText;
    }
    if (!stdoutText.isEmpty() && !isSystemdRunNoise(stdoutText)) {
        return stdoutText;
    }
    const QString last = readFreshLastError(startedAt);
    if (!last.isEmpty()) {
        return last;
    }
    return QObject::tr(
        "xzram operation failed (exit %1; see journalctl -t xzram-helper)").arg(exitCode);
}

} // namespace

namespace XzramCli {

QString findBinary() {
#ifdef XZRAM_DEV_OVERRIDES
    const QByteArray overridePath = qgetenv("XZRAM_CLI");
    if (!overridePath.isEmpty() && qEnvironmentVariableIsSet("XZRAM_ALLOW_DEV_CLI")) {
        const QString path = QString::fromLocal8Bit(overridePath);
        if (QFileInfo(path).isAbsolute() && QFile::exists(path)) {
            return path;
        }
    }
#endif
    // Never resolved via $PATH: the GUI drives privileged operations through this binary.
    return QStringLiteral(XZRAM_CLI_INSTALL_PATH);
}

RunResult resultFromOutput(int exitCode, bool crashed, const QString &stdoutText,
                           const QString &stderrText, const QDateTime &startedAt,
                           int extraOkExitCode) {
    RunResult result;
    result.exitCode = exitCode;
    result.stdoutText = stdoutText;
    result.stderrText = stderrText;
    result.ok = !crashed
        && (exitCode == kExitOk || (extraOkExitCode >= 0 && exitCode == extraOkExitCode));
    if (!result.ok) {
        result.error = bestError(exitCode, stderrText, stdoutText, startedAt);
    }
    return result;
}

RunResult run(const QStringList &args, int timeoutMs, int extraOkExitCode) {
    RunResult result;
    QProcess process;
    process.setProgram(findBinary());
    process.setArguments(args);
    const QDateTime startedAt = QDateTime::currentDateTimeUtc();
    process.start();
    if (!process.waitForStarted(3000)) {
        result.error = QObject::tr("failed to start xzram CLI (%1)").arg(findBinary());
        return result;
    }
    if (!process.waitForFinished(timeoutMs)) {
        process.kill();
        process.waitForFinished(3000);
        result.error = QObject::tr("xzram CLI timed out");
        return result;
    }
    return resultFromOutput(
        process.exitCode(), process.exitStatus() == QProcess::CrashExit,
        QString::fromUtf8(process.readAllStandardOutput()).trimmed(),
        QString::fromUtf8(process.readAllStandardError()).trimmed(), startedAt, extraOkExitCode);
}

QString runJson(const QStringList &args, int timeoutMs, int extraOkExitCode) {
    const RunResult result = run(args, timeoutMs, extraOkExitCode);
    if (!result.ok) {
        QJsonObject obj;
        obj.insert(QStringLiteral("error"), result.error);
        return QString::fromUtf8(QJsonDocument(obj).toJson(QJsonDocument::Compact));
    }
    return result.stdoutText;
}

QString swapfileCheckJson(const QString &path) {
    return runJson({QStringLiteral("swapfile"), QStringLiteral("check"), path,
                    QStringLiteral("--json")});
}

QString pendingJson() {
    return runJson(argsPendingShowJson());
}

QString recommendedDefaultsJson() {
    return recommendedDefaultsJson(QStringLiteral("default"), QStringLiteral("default"));
}

QString recommendedDefaultsJson(const QString &zramScale, const QString &swapScale) {
    return runJson(argsDefaultsRecommend(zramScale, swapScale));
}

QString snapshotsJson() {
    return runJson(argsSnapshotListJson());
}

} // namespace XzramCli
