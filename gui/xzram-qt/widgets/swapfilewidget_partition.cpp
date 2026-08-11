#include "swapfilewidget.h"

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
    QString error;
    if (!XzramCli::swapOn(device, &error)) {
        QMessageBox::warning(this, tr("Enable swap failed"), error);
        return;
    }
    emit refreshRequested();
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
    QString error;
    if (!XzramCli::swapOff(device, &error)) {
        QMessageBox::warning(this, tr("Disable swap failed"), error);
        return;
    }
    emit refreshRequested();
}
