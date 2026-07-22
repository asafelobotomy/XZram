#include "swapfilewidget.h"

#include "jsonloader.h"
#include "xzramcli.h"

#include <QCheckBox>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QTableWidget>

void SwapfileWidget::setDetectionJson(const QString &json) {
    QString error;
    const QJsonObject root = JsonLoader::parseObject(json, &error);
    const QString rootFs = JsonLoader::optionalString(root, QStringLiteral("root_filesystem"));
    m_onBtrfs = rootFs == QLatin1String("btrfs");
    if (m_onBtrfs) {
        m_checkBtrfsButton->show();
        m_prepareBtrfsButton->show();
        m_mkdirCheck->show();
    } else {
        m_btrfsBanner->hide();
        m_btrfsStatus->hide();
        m_checkBtrfsButton->hide();
        m_prepareBtrfsButton->hide();
        m_mkdirCheck->hide();
    }
    updateBtrfsBanner();
}

bool SwapfileWidget::anySwapfileReady() const {
    for (int row = 0; row < m_table->rowCount(); ++row) {
        const QTableWidgetItem *item = m_table->item(row, 0);
        if (!item || item->text().isEmpty()) {
            continue;
        }
        QString parseError;
        const QJsonObject check =
            JsonLoader::parseObject(XzramCli::swapfileCheckJson(item->text()), &parseError);
        if (JsonLoader::optionalBool(check, QStringLiteral("ready"), false)) {
            return true;
        }
    }
    const QString path = m_pathEdit->text().trimmed();
    if (!path.isEmpty()) {
        QString parseError;
        const QJsonObject check =
            JsonLoader::parseObject(XzramCli::swapfileCheckJson(path), &parseError);
        if (JsonLoader::optionalBool(check, QStringLiteral("ready"), false)) {
            return true;
        }
    }
    return false;
}

void SwapfileWidget::updateBtrfsBanner() {
    if (!m_onBtrfs) {
        m_btrfsBanner->hide();
        return;
    }
    if (anySwapfileReady()) {
        m_btrfsBanner->hide();
        return;
    }
    m_btrfsBanner->setText(
        tr("<b>Btrfs detected.</b> New swap files need a prepared parent folder. Use "
           "<i>Check swap readiness</i> then <i>Prepare directory for swap</i> before staging create."));
    m_btrfsBanner->show();
}

void SwapfileWidget::updateBtrfsStatus(const QString &json) {
    QString error;
    const QJsonObject root = JsonLoader::parseObject(json, &error);
    if (root.contains(QStringLiteral("error"))) {
        m_btrfsStatus->setText(root.value(QStringLiteral("error")).toString());
        m_btrfsStatus->setStyleSheet(
            QStringLiteral("color: #721c24; background: #f8d7da; padding: 8px; border-radius: 4px;"));
        m_btrfsStatus->show();
        return;
    }

    const bool ready = JsonLoader::optionalBool(root, QStringLiteral("ready"), false);
    const QString message = JsonLoader::optionalString(root, QStringLiteral("message"));
    m_btrfsStatus->setText(message);
    if (ready) {
        m_btrfsStatus->setStyleSheet(
            QStringLiteral("color: #155724; background: #d4edda; padding: 8px; border-radius: 4px;"));
    } else {
        m_btrfsStatus->setStyleSheet(
            QStringLiteral("color: #856404; background: #fff3cd; padding: 8px; border-radius: 4px;"));
    }
    m_btrfsStatus->show();
}

QString SwapfileWidget::targetPath() const {
    const QString selected = selectedPath();
    if (!selected.isEmpty()) {
        return selected;
    }
    return m_pathEdit->text().trimmed();
}

void SwapfileWidget::checkBtrfs() {
    const QString path = targetPath();
    if (path.isEmpty()) {
        QMessageBox::information(this, tr("Check swap readiness"),
                                 tr("Enter or select a swap file path first."));
        return;
    }
    updateBtrfsStatus(XzramCli::swapfileCheckJson(path));
}

void SwapfileWidget::prepareBtrfs() {
    const QString path = targetPath();
    if (path.isEmpty()) {
        QMessageBox::information(this, tr("Prepare directory"),
                                 tr("Enter or select a swap file path first."));
        return;
    }

    QString error;
    if (!XzramCli::swapfilePrepare(path, m_mkdirCheck->isChecked(), &error)) {
        QMessageBox::warning(this, tr("Prepare failed"), error);
        return;
    }

    updateBtrfsStatus(XzramCli::swapfileCheckJson(path));
    QMessageBox::information(
        this, tr("Prepare complete"),
        tr("This path is ready for a swap file. Review the status message before staging create."));
    updateBtrfsBanner();
}
