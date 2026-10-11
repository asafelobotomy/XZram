#include "clijob.h"

#include <QProcess>
#include <QTimer>

CliJob::CliJob(QObject *parent) : QObject(parent) {
    m_process = new QProcess(this);
    m_timeout = new QTimer(this);
    m_timeout->setSingleShot(true);

    connect(m_process, &QProcess::started, this, &CliJob::onStarted);
    connect(m_process, QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished), this,
            &CliJob::onFinished);
    connect(m_process, &QProcess::errorOccurred, this, &CliJob::onErrorOccurred);
    connect(m_timeout, &QTimer::timeout, this, &CliJob::onTimeout);
}

CliJob::~CliJob() {
    if (m_process->state() != QProcess::NotRunning) {
        m_process->kill();
        m_process->waitForFinished(3000);
    }
}

bool CliJob::isRunning() const {
    return m_process->state() != QProcess::NotRunning;
}

void CliJob::start(const QStringList &args, int timeoutMs, const QByteArray &stdinData) {
    if (isRunning()) {
        return;
    }
    m_emitted = false;
    m_timedOut = false;
    m_canceled = false;
    m_stdinData = stdinData;
    m_startedAt = QDateTime::currentDateTimeUtc();
    m_process->setProgram(XzramCli::findBinary());
    m_process->setArguments(args);
    m_process->start();
    if (timeoutMs > 0) {
        m_timeout->start(timeoutMs);
    }
}

void CliJob::setExtraOkExitCode(int code) {
    m_extraOkExitCode = code;
}

void CliJob::cancel() {
    if (!isRunning()) {
        return;
    }
    m_timeout->stop();
    m_canceled = true;
    m_process->kill();
}

void CliJob::onStarted() {
    if (!m_stdinData.isEmpty()) {
        m_process->write(m_stdinData);
        m_stdinData.clear();
    }
    m_process->closeWriteChannel();
}

void CliJob::onFinished(int exitCode, QProcess::ExitStatus status) {
    m_timeout->stop();
    if (m_timedOut) {
        XzramCli::RunResult result;
        result.error = QStringLiteral("xzram CLI timed out");
        emitFinished(result);
        return;
    }
    if (m_canceled) {
        XzramCli::RunResult result;
        result.error = QStringLiteral("cancelled");
        emitFinished(result);
        return;
    }
    const QString stdoutText =
        QString::fromUtf8(m_process->readAllStandardOutput()).trimmed();
    const QString stderrText =
        QString::fromUtf8(m_process->readAllStandardError()).trimmed();
    emitFinished(XzramCli::resultFromOutput(exitCode, status == QProcess::CrashExit, stdoutText,
                                            stderrText, m_startedAt, m_extraOkExitCode));
}

void CliJob::onErrorOccurred(QProcess::ProcessError error) {
    if (error == QProcess::FailedToStart) {
        m_timeout->stop();
        XzramCli::RunResult result;
        result.error = tr("failed to start xzram CLI (%1)").arg(XzramCli::findBinary());
        emitFinished(result);
    }
}

void CliJob::onTimeout() {
    if (!isRunning()) {
        return;
    }
    m_timedOut = true;
    m_process->kill();
}

void CliJob::emitFinished(const XzramCli::RunResult &result) {
    if (m_emitted) {
        return;
    }
    m_emitted = true;
    emit finished(result);
}
