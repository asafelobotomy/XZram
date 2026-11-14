// MainWindow: pending apply/clear and recommended defaults (privileged ops run async).
#include "mainwindow.h"

#include "jsonloader.h"
#include "privilegedjob.h"
#include "xzramcli.h"
#include "widgets/pendingbanner.h"
#include "widgets/recommendeddefaultsdialog.h"
#include "widgets/settingswidget.h"
#include "widgets/sysctlwidget.h"
#include "widgets/zramwidget.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMessageBox>
#include <QPushButton>
#include <QTabWidget>

namespace {

using XzramCli::ZramRestart;

/// The memory guard refused a zram restart. Offer its two ways forward; Normal = cancel.
ZramRestart askZramRestart(QWidget *parent, const QString &error) {
    QMessageBox box(QMessageBox::Warning, QObject::tr("Not enough free memory"), error,
                    QMessageBox::NoButton, parent);
    box.setInformativeText(QObject::tr(
        "Restarting zram moves its data back into RAM first, and there isn't room for it "
        "right now. You can write the configuration now and let zram pick it up at the next "
        "boot, or restart anyway (the system may stall or kill programs while it does)."));
    auto *atBoot = box.addButton(QObject::tr("Apply at next boot"), QMessageBox::AcceptRole);
    auto *force = box.addButton(QObject::tr("Restart anyway"), QMessageBox::DestructiveRole);
    box.addButton(QMessageBox::Cancel);
    box.setDefaultButton(atBoot);
    box.exec();
    if (box.clickedButton() == atBoot) {
        return ZramRestart::AtBoot;
    }
    if (box.clickedButton() == force) {
        return ZramRestart::Force;
    }
    return ZramRestart::Normal;
}

/// Success text plus what the CLI printed (e.g. the at-boot note or the force warning).
QString withCliMessages(const QString &summary, const XzramCli::RunResult &result) {
    const QString out = result.stdoutText.trimmed();
    return out.isEmpty() ? summary : summary + QStringLiteral("\n\n") + out;
}

} // namespace

void MainWindow::onStagingChanged() {
    updatePendingBanner();
}

void MainWindow::applyPending() {
    if (m_settingsPage->confirmBeforeApply()) {
        const auto answer = QMessageBox::question(
            this, tr("Apply pending"),
            tr("Apply staged configuration changes now?"),
            QMessageBox::Yes | QMessageBox::No, QMessageBox::Yes);
        if (answer != QMessageBox::Yes) {
            return;
        }
    }

    runApply(ZramRestart::Normal);
}

void MainWindow::runApply(ZramRestart mode) {
    runPrivilegedCli(this, tr("Applying configuration…"), XzramCli::argsApply(mode),
                     [this, mode](const XzramCli::RunResult &result) {
                         if (!result.ok) {
                             if (mode == ZramRestart::Normal
                                 && XzramCli::isZramRestartRefusal(result.error)) {
                                 const ZramRestart next = askZramRestart(this, result.error);
                                 if (next != ZramRestart::Normal) {
                                     runApply(next);
                                 }
                                 return;
                             }
                             QMessageBox::warning(this, tr("Apply failed"), result.error);
                             return;
                         }
                         QMessageBox::information(
                             this, tr("Apply"),
                             withCliMessages(tr("Pending configuration applied."), result));
                         refreshAll();
                     });
}

void MainWindow::clearPending() {
    runPrivilegedCli(this, tr("Clearing staged changes…"), XzramCli::argsClearPending(),
                     [this](const XzramCli::RunResult &result) {
                         if (!result.ok) {
                             QMessageBox::warning(this, tr("Clear failed"), result.error);
                             return;
                         }
                         updatePendingBanner();
                     });
}

bool MainWindow::pendingHasChanges(const QJsonObject &pending) const {
    if (pending.isEmpty() || pending.contains(QStringLiteral("error"))) {
        return false;
    }
    if (pending.value(QStringLiteral("zram")).isObject()) {
        return true;
    }
    if (pending.value(QStringLiteral("disable_zram")).toBool()
        || pending.value(QStringLiteral("disable_zswap")).toBool()
        || pending.value(QStringLiteral("enable_oomd")).toBool()
        || !pending.value(QStringLiteral("mm")).toObject().isEmpty()) {
        return true;
    }
    if (!pending.value(QStringLiteral("prepare_swapfiles")).toArray().isEmpty()) {
        return true;
    }
    for (const auto *key : {"swapfile", "swapfile_resize", "remove_swapfile", "sysctl"}) {
        if (!pending.value(QString::fromLatin1(key)).isNull()) {
            return true;
        }
    }
    return false;
}

void MainWindow::previewPendingInTabs(const QJsonObject &pending) {
    if (pending.contains(QStringLiteral("zram"))) {
        const QJsonObject zram = pending.value(QStringLiteral("zram")).toObject();
        m_zramPage->setZramConfigJson(
            QString::fromUtf8(QJsonDocument(zram).toJson(QJsonDocument::Compact)));
    }
    if (pending.contains(QStringLiteral("sysctl"))) {
        const QJsonObject sysctl = pending.value(QStringLiteral("sysctl")).toObject();
        m_sysctlPage->setSysctlJson(
            QString::fromUtf8(QJsonDocument(sysctl).toJson(QJsonDocument::Compact)));
    }
}

void MainWindow::recommendDefaults() {
    const QString json = XzramCli::recommendedDefaultsJson();
    QString parseError;
    const QJsonObject report = JsonLoader::parseObject(json, &parseError);
    if (report.contains(QStringLiteral("error"))) {
        QMessageBox::warning(this, tr("Recommend failed"),
                             report.value(QStringLiteral("error")).toString());
        return;
    }

    const auto result = RecommendedDefaultsDialog::showDialog(this, report);
    if (result.choice == RecommendedDefaultsDialog::Choice::Cancel) {
        return;
    }

    // Re-fetch with chosen scales so pending preview matches Apply/Stage.
    const QString scaledJson =
        XzramCli::recommendedDefaultsJson(result.zramScale, result.swapScale);
    QString scaledParseError;
    const QJsonObject scaledReport = JsonLoader::parseObject(scaledJson, &scaledParseError);
    if (scaledReport.contains(QStringLiteral("error"))) {
        QMessageBox::warning(this, tr("Recommend failed"),
                             scaledReport.value(QStringLiteral("error")).toString());
        return;
    }

    const QJsonObject pending = scaledReport.value(QStringLiteral("pending")).toObject();
    if (!pendingHasChanges(pending)) {
        QMessageBox::information(this, tr("Recommended defaults"),
                                 tr("Your system already matches the recommended defaults."));
        return;
    }

    if (result.choice == RecommendedDefaultsDialog::Choice::ApplyDefaults) {
        runDefaultsApply(result.zramScale, result.swapScale, ZramRestart::Normal);
        return;
    }

    if (result.choice == RecommendedDefaultsDialog::Choice::Configure) {
        stageRecommendedDefaults(result.zramScale, result.swapScale, pending);
    }
}

void MainWindow::runDefaultsApply(const QString &zramScale, const QString &swapScale,
                                  ZramRestart mode) {
    runPrivilegedCli(
        this, tr("Applying recommended defaults…"),
        XzramCli::argsDefaultsApply(zramScale, swapScale, mode),
        [this, zramScale, swapScale, mode](const XzramCli::RunResult &runResult) {
            if (!runResult.ok) {
                if (mode == ZramRestart::Normal
                    && XzramCli::isZramRestartRefusal(runResult.error)) {
                    const ZramRestart next = askZramRestart(this, runResult.error);
                    if (next != ZramRestart::Normal) {
                        runDefaultsApply(zramScale, swapScale, next);
                    }
                    return;
                }
                QMessageBox::warning(this, tr("Apply failed"), runResult.error);
                return;
            }
            QMessageBox::information(
                this, tr("Apply complete"),
                withCliMessages(tr("Recommended defaults have been applied."), runResult));
            refreshAll();
        });
}

void MainWindow::stageRecommendedDefaults(const QString &zramScale, const QString &swapScale,
                                          const QJsonObject &pending) {
    runPrivilegedCli(
        this, tr("Staging recommended defaults…"),
        XzramCli::argsDefaultsStage(zramScale, swapScale),
        [this, pending](const XzramCli::RunResult &runResult) {
            if (!runResult.ok) {
                QMessageBox::warning(this, tr("Stage failed"), runResult.error);
                return;
            }
            previewPendingInTabs(pending);
            updatePendingBanner();
            m_tabs->setCurrentWidget(m_zramPage);
            QMessageBox::information(
                this, tr("Defaults staged"),
                tr("Recommended defaults are staged. Review each tab, adjust if needed, then "
                   "use Apply in the pending banner."));
            // Do not refreshAll() here — it reloads live config and wipes staged tab preview.
        });
}
