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

#include <QJsonDocument>
#include <QJsonObject>
#include <QMessageBox>
#include <QTabWidget>

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

    runPrivilegedCli(this, tr("Applying configuration…"), XzramCli::argsApply(),
                     [this](const XzramCli::RunResult &result) {
                         if (!result.ok) {
                             QMessageBox::warning(this, tr("Apply failed"), result.error);
                             return;
                         }
                         QMessageBox::information(this, tr("Apply"),
                                                  tr("Pending configuration applied."));
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
    if (pending.value(QStringLiteral("disable_zram")).toBool()) {
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
        runPrivilegedCli(
            this, tr("Applying recommended defaults…"),
            XzramCli::argsDefaultsApply(result.zramScale, result.swapScale),
            [this](const XzramCli::RunResult &runResult) {
                if (!runResult.ok) {
                    QMessageBox::warning(this, tr("Apply failed"), runResult.error);
                    return;
                }
                QMessageBox::information(this, tr("Apply complete"),
                                         tr("Recommended defaults have been applied."));
                refreshAll();
            });
        return;
    }

    if (result.choice == RecommendedDefaultsDialog::Choice::Configure) {
        stageRecommendedDefaults(result.zramScale, result.swapScale, pending);
    }
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
