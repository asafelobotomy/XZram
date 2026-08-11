#ifndef CLIJOB_H
#define CLIJOB_H

#include "xzramcli.h"

#include <QByteArray>
#include <QObject>
#include <QProcess>
#include <QString>
#include <QStringList>
#include <functional>

class QProgressDialog;
class QTimer;
class QWidget;

/// Async xzram CLI invocation via QProcess signals (does not block the UI thread).
class CliJob : public QObject {
    Q_OBJECT

public:
    explicit CliJob(QObject *parent = nullptr);
    ~CliJob() override;

    bool isRunning() const;
    void start(const QStringList &args, int timeoutMs = 120000,
               const QByteArray &stdinData = QByteArray());
    void cancel();

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
    bool m_emitted = false;
    bool m_timedOut = false;
    bool m_canceled = false;
};

/// Show a modal indeterminate progress dialog, run args async, invoke onDone on finish.
void runCliWithProgress(QWidget *parent, const QString &label, const QStringList &args,
                        int timeoutMs, const std::function<void(const XzramCli::RunResult &)> &onDone);

#endif
