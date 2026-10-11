// Argv builders for xzram CLI invocations.
#include "xzramcli.h"

namespace XzramCli {

namespace {

QStringList jsonArgs(const QString &group, const QString &command) {
    return {group, command, QStringLiteral("--json")};
}

} // namespace

QStringList argsApply() {
    return {QStringLiteral("apply")};
}

QStringList argsClearPending() {
    return {QStringLiteral("pending"), QStringLiteral("clear")};
}

QStringList argsDefaultsApply(const QString &zramScale, const QString &swapScale) {
    return {QStringLiteral("defaults"), QStringLiteral("apply"), QStringLiteral("--yes"),
            QStringLiteral("--zram-scale"), zramScale, QStringLiteral("--swap-scale"), swapScale};
}

QStringList argsDefaultsStage(const QString &zramScale, const QString &swapScale) {
    return {QStringLiteral("defaults"), QStringLiteral("stage"), QStringLiteral("--zram-scale"),
            zramScale, QStringLiteral("--swap-scale"), swapScale};
}

QStringList argsDefaultsRecommend(const QString &zramScale, const QString &swapScale) {
    return {QStringLiteral("defaults"), QStringLiteral("recommend"), QStringLiteral("--json"),
            QStringLiteral("--zram-scale"), zramScale, QStringLiteral("--swap-scale"), swapScale};
}

QStringList argsDefaultsOptimizeLinked(const QString &anchor) {
    return {QStringLiteral("defaults"), QStringLiteral("optimize-linked"), QStringLiteral("--json"),
            QStringLiteral("--anchor"), anchor};
}

QStringList argsZramSet(const QString &device, const QString &size, const QString &algorithm,
                        int priority) {
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
    return args;
}

QStringList argsZramDisable() {
    return {QStringLiteral("zram"), QStringLiteral("disable")};
}

QStringList argsZramMigrate() {
    return {QStringLiteral("zram"), QStringLiteral("migrate")};
}

QStringList argsSwapfileCreate(const QString &path, quint64 sizeMb, int priority, bool prepare,
                               bool mkdirParents) {
    QStringList args = {QStringLiteral("swapfile"), QStringLiteral("create"), path,
                        QStringLiteral("--size-mb"), QString::number(sizeMb),
                        QStringLiteral("--priority"), QString::number(priority)};
    if (prepare) {
        args.append(QStringLiteral("--prepare"));
        if (mkdirParents) {
            args.append(QStringLiteral("--mkdir"));
        }
    }
    return args;
}

QStringList argsSwapfileResize(const QString &path, quint64 sizeMb) {
    return {QStringLiteral("swapfile"), QStringLiteral("resize"), path,
            QStringLiteral("--size-mb"), QString::number(sizeMb)};
}

QStringList argsSwapfileRemove(const QString &path) {
    return {QStringLiteral("swapfile"), QStringLiteral("remove"), path};
}

QStringList argsSwapfilePrepare(const QString &path, bool mkdirParents) {
    QStringList args = {QStringLiteral("swapfile"), QStringLiteral("prepare"), path};
    if (mkdirParents) {
        args << QStringLiteral("--mkdir");
    }
    return args;
}

QStringList argsSysctlSet(const QStringList &flagArgs) {
    QStringList args = {QStringLiteral("sysctl"), QStringLiteral("set")};
    args.append(flagArgs);
    return args;
}

QStringList argsSwapOn(const QString &device) {
    return {QStringLiteral("swap"), QStringLiteral("on"), device};
}

QStringList argsSwapOff(const QString &device) {
    return {QStringLiteral("swap"), QStringLiteral("off"), device};
}

QStringList argsSnapshotCreate(const QString &label) {
    QStringList args = {QStringLiteral("snapshot"), QStringLiteral("create")};
    if (!label.isEmpty()) {
        args << QStringLiteral("--label") << label;
    }
    return args;
}

QStringList argsSnapshotRestore(const QString &id) {
    return {QStringLiteral("snapshot"), QStringLiteral("restore"), id};
}

QStringList argsSnapshotDelete(const QString &id) {
    return {QStringLiteral("snapshot"), QStringLiteral("delete"), id, QStringLiteral("--yes")};
}

QStringList argsSnapshotPrune(int keep) {
    return {QStringLiteral("snapshot"), QStringLiteral("prune"), QStringLiteral("--keep"),
            QString::number(keep), QStringLiteral("--yes")};
}

QStringList argsRollback() {
    return {QStringLiteral("rollback")};
}

QStringList argsStatusJson() {
    return {QStringLiteral("status"), QStringLiteral("--json")};
}

QStringList argsDetectJson() {
    return {QStringLiteral("detect"), QStringLiteral("--json")};
}

QStringList argsDoctorJson() {
    return {QStringLiteral("doctor"), QStringLiteral("--json")};
}

QStringList argsZramShowJson() {
    return jsonArgs(QStringLiteral("zram"), QStringLiteral("show"));
}

QStringList argsSwapfileListJson() {
    return jsonArgs(QStringLiteral("swapfile"), QStringLiteral("list"));
}

QStringList argsSwapListJson() {
    return jsonArgs(QStringLiteral("swap"), QStringLiteral("list"));
}

QStringList argsSysctlShowJson() {
    return jsonArgs(QStringLiteral("sysctl"), QStringLiteral("show"));
}

QStringList argsPendingShowJson() {
    return jsonArgs(QStringLiteral("pending"), QStringLiteral("show"));
}

QStringList argsSnapshotListJson() {
    return jsonArgs(QStringLiteral("snapshot"), QStringLiteral("list"));
}

} // namespace XzramCli
