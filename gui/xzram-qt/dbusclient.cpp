#include "dbusclient.h"
#include "clifallback.h"

#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusInterface>
#include <QDBusReply>
#include <QFile>
#include <QIODevice>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QStandardPaths>
#include <QThread>
#include <QVariantMap>

namespace {
constexpr auto kObjectPath = "/io/github/XZram";
constexpr auto kInterface = "io.github.XZram.Manager";
constexpr auto kLastErrorPath = "/var/lib/xzram/last_error";

QString jsonError(const QString &message) {
    QJsonObject obj;
    obj.insert(QStringLiteral("error"), message);
    return QString::fromUtf8(QJsonDocument(obj).toJson(QJsonDocument::Compact));
}

QString findHelperBinary() {
    const QStringList candidates = {
        QStringLiteral("/usr/libexec/xzram-helper"),
        QStringLiteral("/usr/local/libexec/xzram-helper"),
        QStandardPaths::locate(QStandardPaths::GenericDataLocation,
                                QStringLiteral("../libexec/xzram-helper")),
    };
    const QString homeHelper = QStringLiteral("%1/.local/libexec/xzram-helper")
                                   .arg(qEnvironmentVariable("HOME"));
    for (const QString &path : candidates) {
        if (!path.isEmpty() && QFile::exists(path)) {
            return path;
        }
    }
    if (QFile::exists(homeHelper)) {
        return homeHelper;
    }
    return QStringLiteral("/usr/libexec/xzram-helper");
}

/// Prefer the helper's last_error file — pkexec/systemd-run often swallows stderr.
QString privilegedHelperError(QProcess &process) {
    QFile lastErrorFile(QString::fromUtf8(kLastErrorPath));
    if (lastErrorFile.open(QIODevice::ReadOnly | QIODevice::Text)) {
        const QString last = QString::fromUtf8(lastErrorFile.readAll()).trimmed();
        if (!last.isEmpty()) {
            return last;
        }
    }

    const QString err = QString::fromUtf8(process.readAllStandardError()).trimmed();
    const QString out = QString::fromUtf8(process.readAllStandardOutput()).trimmed();
    if (err.contains(QStringLiteral("xzram-helper:"))) {
        return err;
    }
    if (out.contains(QStringLiteral("xzram-helper:"))) {
        return out;
    }

    QString combined = err;
    if (!out.isEmpty()) {
        if (!combined.isEmpty()) {
            combined.append(QLatin1Char('\n'));
        }
        combined.append(out);
    }
    if (combined.isEmpty()) {
        return QStringLiteral("privileged helper failed (no error details)");
    }
    return combined;
}
} // namespace

bool DbusClient::isRegistered() const {
    QDBusConnectionInterface *iface = QDBusConnection::systemBus().interface();
    if (!iface) {
        return false;
    }
    const QDBusReply<bool> reply = iface->isServiceRegistered(QString::fromUtf8(kBusName));
    return reply.isValid() && reply.value();
}

bool DbusClient::startService(QString *error) const {
    QDBusConnectionInterface *iface = QDBusConnection::systemBus().interface();
    if (!iface) {
        if (error) {
            *error = QStringLiteral("system D-Bus is not available");
        }
        return false;
    }

    const QDBusReply<void> reply = iface->startService(QString::fromUtf8(kBusName));
    if (!reply.isValid()) {
        if (error) {
            *error = reply.error().message();
        }
        return false;
    }
    return true;
}

bool DbusClient::startServiceViaHelper(QString *error) const {
    const QString helper = findHelperBinary();
    if (!QFile::exists(helper)) {
        if (error) {
            *error = QStringLiteral("xzram-helper not found; install the xzram package");
        }
        return false;
    }

    QProcess process;
    process.setProgram(QStringLiteral("pkexec"));
    process.setArguments({helper, QStringLiteral("daemon.start"), QStringLiteral("{}")});
    process.start();
    if (!process.waitForStarted(3000)) {
        if (error) {
            *error = QStringLiteral("failed to launch pkexec");
        }
        return false;
    }
    if (!process.waitForFinished(120000)) {
        process.kill();
        if (error) {
            *error = QStringLiteral("starting xzramd timed out");
        }
        return false;
    }
    if (process.exitStatus() != QProcess::NormalExit || process.exitCode() != 0) {
        if (error) {
            *error = privilegedHelperError(process);
            if (error->isEmpty()) {
                *error = QStringLiteral("failed to start xzramd service");
            }
        }
        return false;
    }
    return true;
}

bool DbusClient::ensureAvailable(int timeoutMs, QString *error) const {
    if (isRegistered()) {
        return true;
    }

    QString startError;
    if (!startService(&startError)) {
        if (error) {
            *error = startError;
        }
    }

    const int stepMs = 200;
    int waited = 0;
    while (waited < timeoutMs) {
        if (isRegistered()) {
            return true;
        }
        QThread::msleep(static_cast<unsigned long>(stepMs));
        waited += stepMs;
    }

    if (error && error->isEmpty()) {
        *error = QStringLiteral("xzramd did not appear on the system bus");
    }
    return false;
}

QString DbusClient::callJsonMethod(const char *method) const {
    return callJsonMethodWithArgs(method, {});
}

QString DbusClient::callJsonMethodWithArgs(const char *method, const QVariantList &args) const {
    QDBusInterface iface(QString::fromUtf8(kBusName), kObjectPath, kInterface,
                         QDBusConnection::systemBus());
    QDBusReply<QVariantMap> reply = iface.callWithArgumentList(QDBus::Block,
                                                               QString::fromUtf8(method), args);
    if (!reply.isValid()) {
        return jsonError(reply.error().message());
    }
    return reply.value().value(QStringLiteral("json")).toString();
}

bool DbusClient::callVoidMethod(const char *method, const QVariantList &args,
                                QString *error) const {
    QDBusInterface iface(QString::fromUtf8(kBusName), kObjectPath, kInterface,
                         QDBusConnection::systemBus());
    QDBusReply<void> reply = iface.callWithArgumentList(QDBus::Block, QString::fromUtf8(method),
                                                        args);
    if (!reply.isValid()) {
        if (error) {
            *error = reply.error().message();
        }
        return false;
    }
    return true;
}

QString DbusClient::getStatusJson() const { return callJsonMethod("GetStatus"); }
QString DbusClient::getDetectionJson() const { return callJsonMethod("GetDetection"); }
QString DbusClient::getDoctorJson() const { return callJsonMethod("RunDoctor"); }
QString DbusClient::getZramConfigJson() const { return callJsonMethod("GetZramConfig"); }
QString DbusClient::listSwapfilesJson() const { return callJsonMethod("ListSwapfiles"); }
QString DbusClient::listSwapsJson() const { return callJsonMethod("ListSwaps"); }
QString DbusClient::getSysctlJson() const { return callJsonMethod("GetSysctl"); }
QString DbusClient::getPendingJson() const { return callJsonMethod("GetPending"); }

QString DbusClient::getRecommendedDefaultsJson() const {
    return callJsonMethod("GetRecommendedDefaults");
}

bool DbusClient::stageRecommendedDefaults(QString *error) const {
    if (isRegistered()) {
        const QString result = callJsonMethod("StageRecommendedDefaults");
        if (result.contains(QStringLiteral("\"error\""))) {
            if (error) {
                *error = result;
            }
            return false;
        }
        return true;
    }

    const QString helper = findHelperBinary();
    if (!QFile::exists(helper)) {
        if (error) {
            *error = QStringLiteral("xzramd and xzram-helper unavailable");
        }
        return false;
    }

    const QString recommendJson = CliFallback::recommendedDefaultsJson();
    QJsonParseError parseError;
    const QJsonDocument doc = QJsonDocument::fromJson(recommendJson.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError || !doc.isObject()) {
        if (error) {
            *error = QStringLiteral("failed to compute recommended defaults");
        }
        return false;
    }
    const QJsonObject pending = doc.object().value(QStringLiteral("pending")).toObject();
    const QString pendingJson =
        QString::fromUtf8(QJsonDocument(pending).toJson(QJsonDocument::Compact));

    QProcess process;
    process.setProgram(QStringLiteral("pkexec"));
    process.setArguments({helper, QStringLiteral("stage"), pendingJson});
    process.start();
    if (!process.waitForStarted(3000) || !process.waitForFinished(120000)
        || process.exitCode() != 0) {
        if (error) {
            *error = privilegedHelperError(process);
        }
        return false;
    }
    return true;
}

bool DbusClient::configureZram(const QString &configJson, QString *error) const {
    return callVoidMethod("ConfigureZram", {configJson}, error);
}

bool DbusClient::disableZram(QString *error) const {
    return callVoidMethod("DisableZram", {}, error);
}

bool DbusClient::createSwapfile(const QString &path, quint64 sizeMb, int priority,
                                QString *error) const {
    return callVoidMethod("CreateSwapfile", {path, QVariant::fromValue(sizeMb), priority}, error);
}

bool DbusClient::removeSwapfile(const QString &path, QString *error) const {
    return callVoidMethod("RemoveSwapfile", {path}, error);
}

bool DbusClient::resizeSwapfile(const QString &path, quint64 sizeMb, QString *error) const {
    return callVoidMethod("ResizeSwapfile", {path, QVariant::fromValue(sizeMb)}, error);
}

bool DbusClient::setSysctl(const QString &valuesJson, QString *error) const {
    return callVoidMethod("SetSysctl", {valuesJson}, error);
}

bool DbusClient::migrateZram(QString *error) const {
    return callVoidMethod("MigrateZram", {}, error);
}

bool DbusClient::clearPending(QString *error) const {
    return callVoidMethod("ClearPending", {}, error);
}

bool DbusClient::applyPending(QString *error) const {
    if (!isRegistered()) {
        const QString helper = findHelperBinary();
        if (!QFile::exists(helper)) {
            if (error) {
                *error = QStringLiteral("xzramd and xzram-helper unavailable");
            }
            return false;
        }

        QProcess process;
        process.setProgram(QStringLiteral("pkexec"));
        process.setArguments({helper, QStringLiteral("apply"), QStringLiteral("{}")});
        process.start();
        if (!process.waitForStarted(3000) || !process.waitForFinished(120000)
            || process.exitCode() != 0) {
            if (error) {
                *error = privilegedHelperError(process);
            }
            return false;
        }
        return true;
    }

    QDBusInterface iface(QString::fromUtf8(kBusName), kObjectPath, kInterface,
                         QDBusConnection::systemBus());
    QDBusReply<QStringList> reply = iface.call(QStringLiteral("Apply"));
    if (!reply.isValid()) {
        if (error) {
            *error = reply.error().message();
        }
        return false;
    }
    return true;
}

QString DbusClient::checkSwapfileBtrfsJson(const QString &path) const {
    return callJsonMethodWithArgs("CheckSwapfileBtrfs", {path});
}

bool DbusClient::prepareSwapfileBtrfs(const QString &path, bool mkdirParents,
                                      QString *error) const {
    if (isRegistered()) {
        const QString result =
            callJsonMethodWithArgs("PrepareSwapfileBtrfs", {path, mkdirParents});
        if (result.contains(QStringLiteral("\"error\""))) {
            if (error) {
                *error = result;
            }
            return false;
        }
        return true;
    }
    return prepareSwapfileBtrfsViaHelper(path, mkdirParents, error);
}

bool DbusClient::prepareSwapfileBtrfsViaHelper(const QString &path, bool mkdirParents,
                                               QString *error) const {
    const QString helper = findHelperBinary();
    if (!QFile::exists(helper)) {
        if (error) {
            *error = QStringLiteral("xzram-helper not found; install the xzram package");
        }
        return false;
    }

    QJsonObject payload;
    payload.insert(QStringLiteral("path"), path);
    payload.insert(QStringLiteral("mkdir_parents"), mkdirParents);
    const QString payloadJson =
        QString::fromUtf8(QJsonDocument(payload).toJson(QJsonDocument::Compact));

    QProcess process;
    process.setProgram(QStringLiteral("pkexec"));
    process.setArguments({helper, QStringLiteral("swapfile.prepare"), payloadJson});
    process.start();
    if (!process.waitForStarted(3000)) {
        if (error) {
            *error = QStringLiteral("failed to launch pkexec");
        }
        return false;
    }
    if (!process.waitForFinished(120000)) {
        process.kill();
        if (error) {
            *error = QStringLiteral("prepare operation timed out");
        }
        return false;
    }
    if (process.exitStatus() != QProcess::NormalExit || process.exitCode() != 0) {
        if (error) {
            *error = privilegedHelperError(process);
            if (error->isEmpty()) {
                *error = QStringLiteral("failed to prepare btrfs directory for swap");
            }
        }
        return false;
    }
    return true;
}

QString DbusClient::listSnapshotsJson() const {
    if (isRegistered()) {
        return callJsonMethod("ListSnapshots");
    }
    return CliFallback::run({QStringLiteral("snapshot"), QStringLiteral("list"),
                             QStringLiteral("--json")});
}

bool DbusClient::createSnapshot(const QString &trigger, const QString &label,
                                QString *error) const {
    if (isRegistered()) {
        QDBusInterface iface(QString::fromUtf8(kBusName), kObjectPath, kInterface,
                             QDBusConnection::systemBus());
        QDBusReply<QVariantMap> reply =
            iface.call(QStringLiteral("CreateSnapshot"), trigger, label);
        if (!reply.isValid()) {
            if (error) {
                *error = reply.error().message();
            }
            return false;
        }
        return true;
    }

    const QString helper = findHelperBinary();
    if (!QFile::exists(helper)) {
        if (error) {
            *error = QStringLiteral("xzramd and xzram-helper unavailable");
        }
        return false;
    }

    QJsonObject payload;
    payload.insert(QStringLiteral("trigger"), trigger);
    if (!label.isEmpty()) {
        payload.insert(QStringLiteral("label"), label);
    }
    const QString payloadJson =
        QString::fromUtf8(QJsonDocument(payload).toJson(QJsonDocument::Compact));

    QProcess process;
    process.setProgram(QStringLiteral("pkexec"));
    process.setArguments({helper, QStringLiteral("snapshot.create"), payloadJson});
    process.start();
    if (!process.waitForStarted(3000) || !process.waitForFinished(60000)
        || process.exitCode() != 0) {
        if (error) {
            *error = privilegedHelperError(process);
        }
        return false;
    }
    return true;
}

bool DbusClient::restoreSnapshot(const QString &id, QString *error) const {
    if (isRegistered()) {
        QDBusInterface iface(QString::fromUtf8(kBusName), kObjectPath, kInterface,
                             QDBusConnection::systemBus());
        QDBusReply<QStringList> reply = iface.call(QStringLiteral("RestoreSnapshot"), id);
        if (!reply.isValid()) {
            if (error) {
                *error = reply.error().message();
            }
            return false;
        }
        return true;
    }

    const QString helper = findHelperBinary();
    if (!QFile::exists(helper)) {
        if (error) {
            *error = QStringLiteral("xzramd and xzram-helper unavailable");
        }
        return false;
    }

    QJsonObject payload;
    payload.insert(QStringLiteral("id"), id);
    const QString payloadJson =
        QString::fromUtf8(QJsonDocument(payload).toJson(QJsonDocument::Compact));

    QProcess process;
    process.setProgram(QStringLiteral("pkexec"));
    process.setArguments({helper, QStringLiteral("snapshot.restore"), payloadJson});
    process.start();
    if (!process.waitForStarted(3000) || !process.waitForFinished(120000)
        || process.exitCode() != 0) {
        if (error) {
            *error = privilegedHelperError(process);
        }
        return false;
    }
    return true;
}
