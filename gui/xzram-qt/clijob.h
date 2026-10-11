#ifndef CLIJOB_H
#define CLIJOB_H

#include "xzramcli.h"

#include <QByteArray>
#include <QDateTime>
#include <QObject>
#include <QProcess>
#include <QString>
#include <QStringList>

class QTimer;

/// Async xzram CLI invocation via QProcess signals (does not block the UI thread).
class CliJob : public QObject {
    Q_OBJECT

public:
    explicit CliJob(QObject *parent = nullptr);
    ~CliJob() override;

    bool isRunning() const;
    /// timeoutMs <= 0 disables the timeout (required for privileged operations, which
    /// must never be killed mid-flight).
    void start(const QStringList &args, int timeoutMs = 30000,
               const QByteArray &stdinData = QByteArray());
    void cancel();
    /// Additional exit code treated as success (e.g. kExitDoctorUnhealthy for doctor).
    void setExtraOkExitCode(int code);

signals:
    void finished(const XzramCli::RunResult &result);

private slots:
    void onStarted();
    void onFinished(int exitCode, QProcess::ExitStatus status);
    void onErrorOccurred(QProcess::ProcessError error);
    void onTimeout();

private:
    void emitFinished(const XzramCli::RunResult &result);

    QProcess *m_process = nullptr;
    QTimer *m_timeout = nullptr;
    QByteArray m_stdinData;
    QDateTime m_startedAt;
    int m_extraOkExitCode = -1;
    bool m_emitted = false;
    bool m_timedOut = false;
    bool m_canceled = false;
};

#endif
