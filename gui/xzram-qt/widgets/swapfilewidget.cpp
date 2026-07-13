#include "swapfilewidget.h"

#include "clifallback.h"
#include "dbusclient.h"
#include "jsonloader.h"

#include <QCheckBox>
#include <QFileDialog>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QSpinBox>
#include <QTableWidget>
#include <QVBoxLayout>

SwapfileWidget::SwapfileWidget(DbusClient *client, QWidget *parent)
    : QWidget(parent), m_client(client) {
    auto *layout = new QVBoxLayout(this);

    m_btrfsBanner = new QLabel(this);
    m_btrfsBanner->setWordWrap(true);
    m_btrfsBanner->setStyleSheet(
        QStringLiteral("background: #fff3cd; color: #856404; padding: 8px; border-radius: 4px;"));
    m_btrfsBanner->hide();
    layout->addWidget(m_btrfsBanner);

    m_btrfsStatus = new QLabel(this);
    m_btrfsStatus->setWordWrap(true);
    m_btrfsStatus->hide();
    layout->addWidget(m_btrfsStatus);

    auto *btrfsActions = new QHBoxLayout();
    m_checkBtrfsButton = new QPushButton(tr("Check btrfs readiness"), this);
    m_prepareBtrfsButton = new QPushButton(tr("Prepare directory (chattr +C)"), this);
    m_mkdirCheck = new QCheckBox(tr("Create parent directories"), this);
    btrfsActions->addWidget(m_checkBtrfsButton);
    btrfsActions->addWidget(m_prepareBtrfsButton);
    btrfsActions->addWidget(m_mkdirCheck);
    btrfsActions->addStretch();
    layout->addLayout(btrfsActions);
    m_checkBtrfsButton->hide();
    m_prepareBtrfsButton->hide();
    m_mkdirCheck->hide();

    m_unavailableLabel = new QLabel(
        tr("Swap file changes require xzramd. Start the service to create, resize, or remove swap "
           "files."),
        this);
    m_unavailableLabel->setWordWrap(true);
    m_unavailableLabel->hide();
    layout->addWidget(m_unavailableLabel);

    m_table = new QTableWidget(0, 3, this);
    m_table->setHorizontalHeaderLabels({tr("Path"), tr("Size (MiB)"), tr("Priority")});
    m_table->horizontalHeader()->setStretchLastSection(true);
    m_table->horizontalHeader()->setSectionResizeMode(0, QHeaderView::Stretch);
    m_table->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_table->verticalHeader()->setVisible(false);
    layout->addWidget(m_table, 1);

    auto *createGroup = new QGroupBox(tr("Create swap file"), this);
    auto *createLayout = new QFormLayout(createGroup);

    auto *pathRow = new QHBoxLayout();
    m_pathEdit = new QLineEdit(createGroup);
    m_browseButton = new QPushButton(tr("Browse…"), createGroup);
    pathRow->addWidget(m_pathEdit, 1);
    pathRow->addWidget(m_browseButton);
    createLayout->addRow(tr("Path"), pathRow);

    m_sizeSpin = new QSpinBox(createGroup);
    m_sizeSpin->setRange(64, 1024 * 1024);
    m_sizeSpin->setSuffix(tr(" MiB"));
    m_sizeSpin->setValue(4096);
    createLayout->addRow(tr("Size"), m_sizeSpin);

    m_prioritySpin = new QSpinBox(createGroup);
    m_prioritySpin->setRange(-1, 32767);
    m_prioritySpin->setValue(10);
    createLayout->addRow(tr("Priority"), m_prioritySpin);

    m_createButton = new QPushButton(tr("Stage create"), createGroup);
    createLayout->addRow(QString(), m_createButton);

    layout->addWidget(createGroup);

    auto *rowActions = new QHBoxLayout();
    m_resizeButton = new QPushButton(tr("Stage resize"), this);
    m_removeButton = new QPushButton(tr("Stage remove"), this);
    rowActions->addWidget(m_resizeButton);
    rowActions->addWidget(m_removeButton);
    rowActions->addStretch();
    layout->addLayout(rowActions);

    connect(m_browseButton, &QPushButton::clicked, this, &SwapfileWidget::browsePath);
    connect(m_createButton, &QPushButton::clicked, this, &SwapfileWidget::stageCreate);
    connect(m_resizeButton, &QPushButton::clicked, this, &SwapfileWidget::stageResize);
    connect(m_removeButton, &QPushButton::clicked, this, &SwapfileWidget::stageRemove);
    connect(m_checkBtrfsButton, &QPushButton::clicked, this, &SwapfileWidget::checkBtrfs);
    connect(m_prepareBtrfsButton, &QPushButton::clicked, this, &SwapfileWidget::prepareBtrfs);

    setEditingEnabled(false);
}

void SwapfileWidget::setDaemonAvailable(bool available) {
    m_daemonAvailable = available;
    m_unavailableLabel->setVisible(!available);
    setEditingEnabled(available);
}

void SwapfileWidget::setEditingEnabled(bool enabled) {
    m_pathEdit->setEnabled(enabled);
    m_sizeSpin->setEnabled(enabled);
    m_prioritySpin->setEnabled(enabled);
    m_browseButton->setEnabled(enabled);
    m_createButton->setEnabled(enabled);
    m_resizeButton->setEnabled(enabled);
    m_removeButton->setEnabled(enabled);
    m_checkBtrfsButton->setEnabled(true);
    m_prepareBtrfsButton->setEnabled(true);
    m_mkdirCheck->setEnabled(true);
}

void SwapfileWidget::setDetectionJson(const QString &json) {
    QString error;
    const QJsonObject root = JsonLoader::parseObject(json, &error);
    const QString rootFs = JsonLoader::optionalString(root, QStringLiteral("root_filesystem"));
    m_onBtrfs = rootFs == QLatin1String("btrfs");
    if (m_onBtrfs) {
        m_btrfsBanner->setText(
            tr("<b>Btrfs detected.</b> Swap files need nodatacow on the parent directory. Use "
               "<i>Check btrfs readiness</i> then <i>Prepare directory</i> before staging create."));
        m_btrfsBanner->show();
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
}

QString SwapfileWidget::fetchBtrfsCheckJson(const QString &path) const {
    if (m_client->isRegistered()) {
        return m_client->checkSwapfileBtrfsJson(path);
    }
    return CliFallback::swapfileCheckJson(path);
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

void SwapfileWidget::setSwapfilesJson(const QString &json) {
    QJsonParseError parseError;
    const QJsonDocument doc = QJsonDocument::fromJson(json.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError) {
        return;
    }

    if (doc.isObject() && doc.object().contains(QStringLiteral("error"))) {
        m_table->setRowCount(0);
        return;
    }

    const QJsonArray files =
        doc.isArray() ? doc.array() : doc.object().value(QStringLiteral("swapfiles")).toArray();
    populateTable(files);
}

void SwapfileWidget::populateTable(const QJsonArray &files) {
    m_table->setRowCount(files.size());
    int row = 0;
    for (const QJsonValue &value : files) {
        const QJsonObject entry = value.toObject();
        m_table->setItem(
            row, 0,
            new QTableWidgetItem(JsonLoader::optionalString(entry, QStringLiteral("path"))));
        m_table->setItem(row, 1,
                         new QTableWidgetItem(QString::number(
                             JsonLoader::optionalUInt64(entry, QStringLiteral("size_mb")))));
        m_table->setItem(row, 2,
                         new QTableWidgetItem(QString::number(
                             JsonLoader::optionalInt(entry, QStringLiteral("priority")))));
        ++row;
    }
}

QString SwapfileWidget::selectedPath() const {
    const auto rows = m_table->selectionModel()->selectedRows();
    if (rows.isEmpty()) {
        return {};
    }
    const QTableWidgetItem *item = m_table->item(rows.first().row(), 0);
    return item ? item->text() : QString();
}

QString SwapfileWidget::targetPath() const {
    const QString selected = selectedPath();
    if (!selected.isEmpty()) {
        return selected;
    }
    return m_pathEdit->text().trimmed();
}

void SwapfileWidget::browsePath() {
    const QString path = QFileDialog::getSaveFileName(this, tr("Swap file path"));
    if (!path.isEmpty()) {
        m_pathEdit->setText(path);
        if (m_onBtrfs) {
            checkBtrfs();
        }
    }
}

void SwapfileWidget::checkBtrfs() {
    const QString path = targetPath();
    if (path.isEmpty()) {
        QMessageBox::information(this, tr("Check btrfs"),
                                 tr("Enter or select a swap file path first."));
        return;
    }
    updateBtrfsStatus(fetchBtrfsCheckJson(path));
}

void SwapfileWidget::prepareBtrfs() {
    const QString path = targetPath();
    if (path.isEmpty()) {
        QMessageBox::information(this, tr("Prepare btrfs"),
                                 tr("Enter or select a swap file path first."));
        return;
    }

    QString error;
    if (!m_client->prepareSwapfileBtrfs(path, m_mkdirCheck->isChecked(), &error)) {
        QMessageBox::warning(this, tr("Prepare failed"), error);
        return;
    }

    updateBtrfsStatus(fetchBtrfsCheckJson(path));
    QMessageBox::information(
        this, tr("Prepare complete"),
        tr("Btrfs nodatacow has been applied. Review the status message before staging create."));
}

void SwapfileWidget::stageCreate() {
    if (!m_daemonAvailable || m_pathEdit->text().isEmpty()) {
        return;
    }
    if (m_onBtrfs) {
        const QString check = fetchBtrfsCheckJson(m_pathEdit->text());
        QString parseError;
        const QJsonObject root = JsonLoader::parseObject(check, &parseError);
        if (!JsonLoader::optionalBool(root, QStringLiteral("ready"), false)) {
            const auto answer = QMessageBox::question(
                this, tr("Btrfs not ready"),
                tr("This path is not nodatacow-ready. Prepare the directory first?\n\n%1")
                    .arg(JsonLoader::optionalString(root, QStringLiteral("message"))));
            if (answer == QMessageBox::Yes) {
                prepareBtrfs();
            }
            return;
        }
    }
    QString error;
    if (!m_client->createSwapfile(m_pathEdit->text(), static_cast<quint64>(m_sizeSpin->value()),
                                  m_prioritySpin->value(), &error)) {
        QMessageBox::warning(this, tr("Stage failed"), error);
        return;
    }
    emit stagingChanged();
}

void SwapfileWidget::stageResize() {
    if (!m_daemonAvailable) {
        return;
    }
    const QString path = selectedPath();
    if (path.isEmpty()) {
        QMessageBox::information(this, tr("Resize"), tr("Select a swap file row first."));
        return;
    }
    QString error;
    if (!m_client->resizeSwapfile(path, static_cast<quint64>(m_sizeSpin->value()), &error)) {
        QMessageBox::warning(this, tr("Stage failed"), error);
        return;
    }
    emit stagingChanged();
}

void SwapfileWidget::stageRemove() {
    if (!m_daemonAvailable) {
        return;
    }
    const QString path = selectedPath();
    if (path.isEmpty()) {
        QMessageBox::information(this, tr("Remove"), tr("Select a swap file row first."));
        return;
    }
    const auto answer =
        QMessageBox::question(this, tr("Remove swap file"), tr("Stage removal of %1?").arg(path));
    if (answer != QMessageBox::Yes) {
        return;
    }
    QString error;
    if (!m_client->removeSwapfile(path, &error)) {
        QMessageBox::warning(this, tr("Stage failed"), error);
        return;
    }
    emit stagingChanged();
}
