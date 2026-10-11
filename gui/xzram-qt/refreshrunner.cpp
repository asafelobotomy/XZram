#include "refreshrunner.h"

#include "clijob.h"
#include "xzramcli.h"

#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QTimer>

namespace {

constexpr int kReadTimeoutMs = 30000;
constexpr int kSystemctlTimeoutMs = 3000;

QStringList argsFor(RefreshRunner::Query query) {
    using Q = RefreshRunner::Query;
    switch (query) {
    case Q::Status:
        return XzramCli::argsStatusJson();
    case Q::Detect:
        return XzramCli::argsDetectJson();
    case Q::Doctor:
        return XzramCli::argsDoctorJson();
    case Q::Swaps:
        return XzramCli::argsSwapListJson();
    case Q::ZramConfig:
        return XzramCli::argsZramShowJson();
    case Q::Swapfiles:
        return XzramCli::argsSwapfileListJson();
    case Q::Sysctl:
        return XzramCli::argsSysctlShowJson();
    case Q::Pending:
        return XzramCli::argsPendingShowJson();
    case Q::Snapshots:
        return XzramCli::argsSnapshotListJson();
    }
    return {};
}

QString errorJson(const QString &error) {
    QJsonObject obj;
    obj.insert(QStringLiteral("error"), error);
    return QString::fromUtf8(QJsonDocument(obj).toJson(QJsonDocument::Compact));
}

} // namespace

RefreshRunner::RefreshRunner(QObject *parent) : QObject(parent) {}

bool RefreshRunner::start(const QList<Query> &queries) {
    if (isRunning()) {
        return false;
    }
    m_results = Results();
    // +1 for the daemon check, +1 held until every job is launched (released below).
    m_outstanding = static_cast<int>(queries.size()) + 2;
    for (const Query query : queries) {
        startQuery(query);
    }
    startDaemonCheck();
    completeOne();
    return true;
}

void RefreshRunner::startQuery(Query query) {
    auto *job = new CliJob(this);
    if (query == Query::Doctor) {
        // Unhealthy systems still produce a valid doctor report.
        job->setExtraOkExitCode(XzramCli::kExitDoctorUnhealthy);
    }
    connect(job, &CliJob::finished, this, [this, job, query](const XzramCli::RunResult &result) {
        job->deleteLater();
        m_results.json.insert(static_cast<int>(query),
                              result.ok ? result.stdoutText : errorJson(result.error));
        completeOne();
    });
    job->start(argsFor(query), kReadTimeoutMs);
}

void RefreshRunner::startDaemonCheck() {
    auto *process = new QProcess(this);
    auto *timer = new QTimer(process);
    timer->setSingleShot(true);
    connect(timer, &QTimer::timeout, process, [process]() { process->kill(); });
    connect(process, QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished), this,
            [this, process, timer](int exitCode, QProcess::ExitStatus status) {
                timer->stop();
                m_results.daemonActive = status == QProcess::NormalExit && exitCode == 0;
                process->deleteLater();
                completeOne();
            });
    connect(process, &QProcess::errorOccurred, this,
            [this, process, timer](QProcess::ProcessError error) {
                if (error != QProcess::FailedToStart) {
                    return;
                }
                timer->stop();
                m_results.daemonActive = false;
                process->deleteLater();
                completeOne();
            });
    process->start(QStringLiteral("systemctl"),
                   {QStringLiteral("is-active"), QStringLiteral("--quiet"),
                    QStringLiteral("xzramd.service")});
    timer->start(kSystemctlTimeoutMs);
}

void RefreshRunner::completeOne() {
    if (m_outstanding <= 0) {
        return;
    }
    if (--m_outstanding == 0) {
        emit finished(m_results);
    }
}
