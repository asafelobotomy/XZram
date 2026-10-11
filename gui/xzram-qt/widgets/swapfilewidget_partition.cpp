#include "swapfilewidget.h"

#include "privilegedjob.h"
#include "xzramcli.h"

#include <QMessageBox>
#include <QString>

void SwapfileWidget::swapOnSelected() {
    const QString device = selectedPartitionDevice();
    if (device.isEmpty()) {
        QMessageBox::information(this, tr("Enable swap"), tr("Select a swap partition first."));
        return;
    }
    const auto reply = QMessageBox::question(
        this, tr("Enable swap"),
        tr("Turn on swap on %1 now? Administrator authentication is required.").arg(device));
    if (reply != QMessageBox::Yes) {
        return;
    }
    runPrivilegedCli(this, tr("Enabling swap on %1…").arg(device), XzramCli::argsSwapOn(device),
                     [this](const XzramCli::RunResult &result) {
                         if (!result.ok) {
                             QMessageBox::warning(this, tr("Enable swap failed"), result.error);
                             return;
                         }
                         emit refreshRequested();
                     });
}

void SwapfileWidget::swapOffSelected() {
    const QString device = selectedPartitionDevice();
    if (device.isEmpty()) {
        QMessageBox::information(this, tr("Disable swap"), tr("Select a swap partition first."));
        return;
    }
    const auto reply = QMessageBox::question(
        this, tr("Disable swap"),
        tr("Turn off swap on %1 now? Active swap use may be disrupted. "
           "Administrator authentication is required.")
            .arg(device),
        QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
    if (reply != QMessageBox::Yes) {
        return;
    }
    runPrivilegedCli(this, tr("Disabling swap on %1…").arg(device),
                     XzramCli::argsSwapOff(device), [this](const XzramCli::RunResult &result) {
                         if (!result.ok) {
                             QMessageBox::warning(this, tr("Disable swap failed"),
                                                  result.error);
                             return;
                         }
                         emit refreshRequested();
                     });
}
