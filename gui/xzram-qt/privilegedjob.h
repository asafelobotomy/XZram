#ifndef PRIVILEGEDJOB_H
#define PRIVILEGEDJOB_H

#include "xzramcli.h"

#include <QString>
#include <QStringList>
#include <functional>

class QWidget;

/// Run a privileged (pkexec / xzramd backed) xzram CLI command without blocking the UI.
///
/// Shows a window-modal busy dialog over `parent`'s window (which disables its controls)
/// until the CLI exits. There is no timeout and no cancel: killing the CLI mid-operation
/// would orphan pkexec/xzram-helper and leave the outcome unknown. `onDone` runs on the
/// UI thread when the CLI exits.
void runPrivilegedCli(QWidget *parent, const QString &label, const QStringList &args,
                      const std::function<void(const XzramCli::RunResult &)> &onDone);

/// Number of privileged CLI operations currently in flight (the window refuses to close
/// while this is non-zero).
int privilegedJobsRunning();

#endif
