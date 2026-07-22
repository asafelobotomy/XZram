#include "xzramcli.h"

#include <QFile>
#include <QIODevice>
#include <QProcess>
#include <QStandardPaths>

namespace {

constexpr auto kLastErrorPath = "/var/lib/xzram/last_error";

QString readLastErrorFile() {
    QFile file(QString::fromUtf8(kLastErrorPath));
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

QString bestError(const QString &stderrText, const QString &stdoutText) {
    const QString last = readLastErrorFile();
    if (!last.isEmpty()) {
        return last;
    }
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
    if (!last.isEmpty()) {
        return last;
    }
    return QStringLiteral(
        "xzram apply failed (see /var/lib/xzram/last_error or journalctl -t xzram-helper)");
}

} // namespace

namespace XzramCli {

QString findBinary() {
    const QByteArray overridePath = qgetenv("XZRAM_CLI");
    if (!overridePath.isEmpty()) {
        const QString path = QString::fromLocal8Bit(overridePath);
        if (QFile::exists(path)) {
            return path;
        }
    }
    const QString path = QStandardPaths::findExecutable(QStringLiteral("xzram"));
    if (!path.isEmpty()) {
        return path;
    }
    return QStringLiteral("xzram");
}

RunResult run(const QStringList &args, int timeoutMs) {
    RunResult result;
    QProcess process;
    process.setProgram(findBinary());
    process.setArguments(args);
    process.start();
    if (!process.waitForStarted(3000)) {
        result.error = QStringLiteral("failed to start xzram CLI");
        return result;
    }
    if (!process.waitForFinished(timeoutMs)) {
        process.kill();
        process.waitForFinished(3000);
        result.error = QStringLiteral("xzram CLI timed out");
        return result;
    }
    result.exitCode = process.exitCode();
    result.stdoutText = QString::fromUtf8(process.readAllStandardOutput()).trimmed();
    result.stderrText = QString::fromUtf8(process.readAllStandardError()).trimmed();
    result.ok = process.exitStatus() == QProcess::NormalExit && result.exitCode == 0;
    if (!result.ok) {
        result.error = bestError(result.stderrText, result.stdoutText);
    }
    return result;
}

QString runJson(const QStringList &args, int timeoutMs) {
    const RunResult result = run(args, timeoutMs);
    if (!result.ok) {
        QString err = result.error;
        err.replace(QLatin1Char('"'), QStringLiteral("\\\""));
        return QStringLiteral("{\"error\":\"%1\"}").arg(err);
    }
    return result.stdoutText;
}

bool runOk(const QStringList &args, QString *error, int timeoutMs) {
    const RunResult result = run(args, timeoutMs);
    if (!result.ok && error) {
        *error = result.error;
    }
    return result.ok;
}

QString statusJson() {
    return runJson({QStringLiteral("status"), QStringLiteral("--json")});
}

QString detectionJson() {
    return runJson({QStringLiteral("detect"), QStringLiteral("--json")});
}

QString doctorJson() {
    return runJson({QStringLiteral("doctor"), QStringLiteral("--json")});
}

QString zramConfigJson() {
    return runJson(
        {QStringLiteral("zram"), QStringLiteral("show"), QStringLiteral("--json")});
}

QString swapfilesJson() {
    return runJson(
        {QStringLiteral("swapfile"), QStringLiteral("list"), QStringLiteral("--json")});
}

QString swapfileCheckJson(const QString &path) {
    return runJson({QStringLiteral("swapfile"), QStringLiteral("check"), path,
                    QStringLiteral("--json")});
}

QString swapsJson() {
    return runJson({QStringLiteral("swap"), QStringLiteral("list"), QStringLiteral("--json")});
}

QString sysctlJson() {
    return runJson(
        {QStringLiteral("sysctl"), QStringLiteral("show"), QStringLiteral("--json")});
}

QString pendingJson() {
    return runJson(
        {QStringLiteral("pending"), QStringLiteral("show"), QStringLiteral("--json")});
}

QString recommendedDefaultsJson() {
    return runJson({QStringLiteral("defaults"), QStringLiteral("recommend"),
                    QStringLiteral("--json")});
}

QString snapshotsJson() {
    return runJson(
        {QStringLiteral("snapshot"), QStringLiteral("list"), QStringLiteral("--json")});
}

bool apply(QString *error) {
    return runOk({QStringLiteral("apply")}, error, 300000);
}

bool clearPending(QString *error) {
    return runOk({QStringLiteral("pending"), QStringLiteral("clear")}, error);
}

bool daemonStart(QString *error) {
    return runOk({QStringLiteral("daemon"), QStringLiteral("start")}, error);
}

bool daemonIsActive() {
    QProcess process;
    process.start(QStringLiteral("systemctl"),
                  {QStringLiteral("is-active"), QStringLiteral("--quiet"),
                   QStringLiteral("xzramd.service")});
    if (!process.waitForFinished(3000)) {
        process.kill();
        process.waitForFinished(1000);
        return false;
    }
    return process.exitStatus() == QProcess::NormalExit && process.exitCode() == 0;
}

bool defaultsStage(QString *error) {
    return runOk({QStringLiteral("defaults"), QStringLiteral("stage")}, error);
}

bool defaultsApply(QString *error) {
    return runOk(
        {QStringLiteral("defaults"), QStringLiteral("apply"), QStringLiteral("--yes")},
        error, 300000);
}

bool zramSet(const QString &device, const QString &size, const QString &algorithm, int priority,
             QString *error) {
    QStringList args = {QStringLiteral("zram"), QStringLiteral("set")};
    if (!device.isEmpty()) {
        args << QStringLiteral("--device") << device;
    }
    if (!size.isEmpty()) {
        args << QStringLiteral("--size") << size;
    }
    if (!algorithm.isEmpty()) {
        args << QStringLiteral("--algorithm") << algorithm;
    }
    args << QStringLiteral("--priority") << QString::number(priority);
    return runOk(args, error);
}

bool zramDisable(QString *error) {
    return runOk({QStringLiteral("zram"), QStringLiteral("disable")}, error);
}

bool zramMigrate(QString *error) {
    return runOk({QStringLiteral("zram"), QStringLiteral("migrate")}, error);
}

bool swapfileCreate(const QString &path, quint64 sizeMb, int priority, QString *error) {
    return runOk({QStringLiteral("swapfile"), QStringLiteral("create"), path,
                  QStringLiteral("--size-mb"), QString::number(sizeMb),
                  QStringLiteral("--priority"), QString::number(priority)},
                 error, 300000);
}

bool swapfileResize(const QString &path, quint64 sizeMb, QString *error) {
    return runOk({QStringLiteral("swapfile"), QStringLiteral("resize"), path,
                  QStringLiteral("--size-mb"), QString::number(sizeMb)},
                 error, 300000);
}

bool swapfileRemove(const QString &path, QString *error) {
    return runOk({QStringLiteral("swapfile"), QStringLiteral("remove"), path}, error);
}

bool swapfilePrepare(const QString &path, bool mkdirParents, QString *error) {
    QStringList args = {QStringLiteral("swapfile"), QStringLiteral("prepare"), path};
    if (mkdirParents) {
        args << QStringLiteral("--mkdir");
    }
    return runOk(args, error);
}

bool sysctlSet(const QStringList &flagArgs, QString *error) {
    QStringList args = {QStringLiteral("sysctl"), QStringLiteral("set")};
    args.append(flagArgs);
    return runOk(args, error);
}

bool swapOn(const QString &device, QString *error) {
    return runOk({QStringLiteral("swap"), QStringLiteral("on"), device}, error);
}

bool swapOff(const QString &device, QString *error) {
    return runOk({QStringLiteral("swap"), QStringLiteral("off"), device}, error);
}

bool snapshotCreate(const QString &label, QString *error) {
    QStringList args = {QStringLiteral("snapshot"), QStringLiteral("create")};
    if (!label.isEmpty()) {
        args << QStringLiteral("--label") << label;
    }
    return runOk(args, error);
}

bool snapshotRestore(const QString &id, QString *error) {
    return runOk({QStringLiteral("snapshot"), QStringLiteral("restore"), id}, error, 300000);
}

bool snapshotDelete(const QString &id, QString *error) {
    return runOk({QStringLiteral("snapshot"), QStringLiteral("delete"), id, QStringLiteral("--yes")},
                 error);
}

bool snapshotPrune(int keep, QString *error) {
    return runOk({QStringLiteral("snapshot"), QStringLiteral("prune"), QStringLiteral("--keep"),
                  QString::number(keep), QStringLiteral("--yes")},
                 error);
}

bool rollback(QString *error) {
    return runOk({QStringLiteral("rollback")}, error, 300000);
}

} // namespace XzramCli
