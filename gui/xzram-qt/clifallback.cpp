#include "clifallback.h"

#include <QProcess>
#include <QStandardPaths>

namespace {

QString findXzramBinary() {
    const QString path = QStandardPaths::findExecutable(QStringLiteral("xzram"));
    if (!path.isEmpty()) {
        return path;
    }
    return QStringLiteral("xzram");
}

QString runCommand(const QStringList &args, int timeoutMs) {
    QProcess process;
    process.setProgram(findXzramBinary());
    process.setArguments(args);
    process.start();
    if (!process.waitForStarted(3000)) {
        return QStringLiteral("{\"error\":\"failed to start xzram CLI\"}");
    }
    if (!process.waitForFinished(timeoutMs)) {
        process.kill();
        return QStringLiteral("{\"error\":\"xzram CLI timed out\"}");
    }
    if (process.exitStatus() != QProcess::NormalExit || process.exitCode() != 0) {
        const QString stderrOut = QString::fromUtf8(process.readAllStandardError()).trimmed();
        return QStringLiteral("{\"error\":\"%1\"}").arg(stderrOut.isEmpty()
            ? QStringLiteral("xzram CLI failed")
            : stderrOut);
    }
    return QString::fromUtf8(process.readAllStandardOutput()).trimmed();
}

} // namespace

namespace CliFallback {

QString run(const QStringList &args, int timeoutMs) {
    return runCommand(args, timeoutMs);
}

QString statusJson() {
    return run({QStringLiteral("status"), QStringLiteral("--json")});
}

QString detectionJson() {
    return run({QStringLiteral("detect"), QStringLiteral("--json")});
}

QString doctorJson() {
    return run({QStringLiteral("doctor"), QStringLiteral("--json")});
}

QString zramConfigJson() {
    return run({QStringLiteral("zram"), QStringLiteral("show"), QStringLiteral("--json")});
}

QString swapfilesJson() {
    return run({QStringLiteral("swapfile"), QStringLiteral("list"), QStringLiteral("--json")});
}

QString swapfileCheckJson(const QString &path) {
    return run({QStringLiteral("swapfile"), QStringLiteral("check"), path, QStringLiteral("--json")});
}

QString swapsJson() {
    return run({QStringLiteral("swap"), QStringLiteral("list"), QStringLiteral("--json")});
}

QString sysctlJson() {
    return run({QStringLiteral("sysctl"), QStringLiteral("show"), QStringLiteral("--json")});
}

QString pendingJson() {
    return run({QStringLiteral("pending"), QStringLiteral("show"), QStringLiteral("--json")});
}

QString recommendedDefaultsJson() {
    return run({QStringLiteral("defaults"), QStringLiteral("recommend"), QStringLiteral("--json")});
}

} // namespace CliFallback
