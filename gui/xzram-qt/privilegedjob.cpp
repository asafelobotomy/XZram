#include "privilegedjob.h"

#include "clijob.h"

#include <QCloseEvent>
#include <QKeyEvent>
#include <QObject>
#include <QProgressDialog>
#include <QWidget>

namespace {

int g_running = 0;

/// Busy indicator that cannot be dismissed (no cancel button, Esc/close ignored).
class BusyDialog : public QProgressDialog {
public:
    BusyDialog(const QString &label, QWidget *parent)
        : QProgressDialog(label, QString(), 0, 0, parent) {
        setCancelButton(nullptr);
        setWindowModality(Qt::WindowModal);
        setMinimumDuration(0);
        setAutoClose(false);
        setAutoReset(false);
        setWindowFlag(Qt::WindowCloseButtonHint, false);
        setValue(0);
    }

    void reject() override {}

protected:
    void closeEvent(QCloseEvent *event) override { event->ignore(); }
    void keyPressEvent(QKeyEvent *event) override {
        if (event->key() == Qt::Key_Escape) {
            event->ignore();
            return;
        }
        QProgressDialog::keyPressEvent(event);
    }
};

} // namespace

void runPrivilegedCli(QWidget *parent, const QString &label, const QStringList &args,
                      const std::function<void(const XzramCli::RunResult &)> &onDone) {
    auto *dialog = new BusyDialog(label, parent);
    auto *job = new CliJob(dialog);
    ++g_running;
    QObject::connect(job, &CliJob::finished, dialog,
                     [dialog, onDone](const XzramCli::RunResult &result) {
                         --g_running;
                         dialog->hide();
                         dialog->deleteLater();
                         if (onDone) {
                             onDone(result);
                         }
                     });
    dialog->show();
    job->start(args, 0);
}

int privilegedJobsRunning() {
    return g_running;
}
