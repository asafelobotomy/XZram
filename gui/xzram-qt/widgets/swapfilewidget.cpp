#include "swapfilewidget.h"

#include "jsonloader.h"
#include "xzramcli.h"

#include <QCheckBox>
#include <QFileDialog>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QItemSelectionModel>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QSpinBox>
#include <QTableWidget>
#include <QVBoxLayout>

SwapfileWidget::SwapfileWidget(QWidget *parent) : QWidget(parent) {
    auto *layout = new QVBoxLayout(this);

    m_introLabel = new QLabel(
        tr("Manage disk overflow swap files and partition swap. Queue create, resize, or "
           "remove, then use Apply now in the banner at the top."),
        this);
    m_introLabel->setWordWrap(true);
    m_introLabel->setStyleSheet(QStringLiteral("color: #495057;"));
    layout->addWidget(m_introLabel);

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
    m_checkBtrfsButton = new QPushButton(tr("Check swap readiness"), this);
    m_prepareBtrfsButton = new QPushButton(tr("Prepare directory for swap"), this);
    m_mkdirCheck = new QCheckBox(tr("Create parent directories"), this);
    m_checkBtrfsButton->setToolTip(
        tr("Check whether this path’s folder is ready for a swap file on btrfs."));
    m_prepareBtrfsButton->setToolTip(
        tr("Mark the parent folder so a swap file can be created safely on btrfs."));
    m_mkdirCheck->setToolTip(tr("Create missing folders on the path when preparing."));
    btrfsActions->addWidget(m_checkBtrfsButton);
    btrfsActions->addWidget(m_prepareBtrfsButton);
    btrfsActions->addWidget(m_mkdirCheck);
    btrfsActions->addStretch();
    layout->addLayout(btrfsActions);
    m_checkBtrfsButton->hide();
    m_prepareBtrfsButton->hide();
    m_mkdirCheck->hide();

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
    m_browseButton->setToolTip(tr("Choose where to create the swap file."));
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

    m_createButton = new QPushButton(tr("Stage new swap file"), createGroup);
    m_createButton->setToolTip(
        tr("Queue creating this swap file. It is written only after you click Apply now in the banner."));
    createLayout->addRow(QString(), m_createButton);

    layout->addWidget(createGroup);

    auto *rowActions = new QHBoxLayout();
    m_resizeButton = new QPushButton(tr("Stage resize"), this);
    m_removeButton = new QPushButton(tr("Stage remove"), this);
    m_resizeButton->setToolTip(
        tr("Queue resizing the selected swap file. Apply now in the banner to finish."));
    m_removeButton->setToolTip(
        tr("Queue deleting the selected swap file. Apply now in the banner to finish."));
    rowActions->addWidget(m_resizeButton);
    rowActions->addWidget(m_removeButton);
    rowActions->addStretch();
    layout->addLayout(rowActions);

    auto *partitionGroup = new QGroupBox(tr("Swap partitions"), this);
    auto *partitionLayout = new QVBoxLayout(partitionGroup);
    m_partitionTable = new QTableWidget(0, 3, partitionGroup);
    m_partitionTable->setHorizontalHeaderLabels({tr("Device"), tr("Status"), tr("Priority")});
    m_partitionTable->horizontalHeader()->setStretchLastSection(true);
    m_partitionTable->horizontalHeader()->setSectionResizeMode(0, QHeaderView::Stretch);
    m_partitionTable->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_partitionTable->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_partitionTable->verticalHeader()->setVisible(false);
    partitionLayout->addWidget(m_partitionTable);
    auto *partitionButtons = new QHBoxLayout();
    m_swapOnButton = new QPushButton(tr("Enable swap"), partitionGroup);
    m_swapOffButton = new QPushButton(tr("Disable swap"), partitionGroup);
    m_swapOnButton->setToolTip(tr("Turn on the selected swap partition right away."));
    m_swapOffButton->setToolTip(tr("Turn off the selected swap partition right away."));
    partitionButtons->addWidget(m_swapOnButton);
    partitionButtons->addWidget(m_swapOffButton);
    partitionButtons->addStretch();
    partitionLayout->addLayout(partitionButtons);
    layout->addWidget(partitionGroup);

    connect(m_browseButton, &QPushButton::clicked, this, &SwapfileWidget::browsePath);
    connect(m_createButton, &QPushButton::clicked, this, &SwapfileWidget::stageCreate);
    connect(m_resizeButton, &QPushButton::clicked, this, &SwapfileWidget::stageResize);
    connect(m_removeButton, &QPushButton::clicked, this, &SwapfileWidget::stageRemove);
    connect(m_checkBtrfsButton, &QPushButton::clicked, this, &SwapfileWidget::checkBtrfs);
    connect(m_prepareBtrfsButton, &QPushButton::clicked, this, &SwapfileWidget::prepareBtrfs);
    connect(m_swapOnButton, &QPushButton::clicked, this, &SwapfileWidget::swapOnSelected);
    connect(m_swapOffButton, &QPushButton::clicked, this, &SwapfileWidget::swapOffSelected);
    connect(m_pathEdit, &QLineEdit::textChanged, this, &SwapfileWidget::updateActionEnabled);
    connect(m_sizeSpin, QOverload<int>::of(&QSpinBox::valueChanged), this,
            &SwapfileWidget::updateActionEnabled);
    connect(m_prioritySpin, QOverload<int>::of(&QSpinBox::valueChanged), this,
            &SwapfileWidget::updateActionEnabled);
    connect(m_table->selectionModel(), &QItemSelectionModel::selectionChanged, this,
            [this]() { updateActionEnabled(); });
    connect(m_partitionTable->selectionModel(), &QItemSelectionModel::selectionChanged, this,
            [this]() { updateActionEnabled(); });

    captureCreateBaseline();
    updateActionEnabled();
}

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

void SwapfileWidget::captureCreateBaseline() {
    m_baselineCreatePath = m_pathEdit->text().trimmed();
    m_baselineCreateSizeMb = static_cast<quint64>(m_sizeSpin->value());
    m_baselineCreatePriority = m_prioritySpin->value();
}

bool SwapfileWidget::createFormDirty() const {
    const QString path = m_pathEdit->text().trimmed();
    if (path.isEmpty()) {
        return false;
    }
    return path != m_baselineCreatePath
        || static_cast<quint64>(m_sizeSpin->value()) != m_baselineCreateSizeMb
        || m_prioritySpin->value() != m_baselineCreatePriority;
}

void SwapfileWidget::updateActionEnabled() {
    m_createButton->setEnabled(createFormDirty());
    const bool hasFileRow = !selectedPath().isEmpty();
    m_resizeButton->setEnabled(hasFileRow);
    m_removeButton->setEnabled(hasFileRow);
    const bool hasPartition = !selectedPartitionDevice().isEmpty();
    m_swapOnButton->setEnabled(hasPartition);
    m_swapOffButton->setEnabled(hasPartition);
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
    updateBtrfsBanner();
}

void SwapfileWidget::setSwapsJson(const QString &json) {
    QJsonParseError parseError;
    const QJsonDocument doc = QJsonDocument::fromJson(json.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError) {
        m_partitionTable->setRowCount(0);
        return;
    }
    if (doc.isObject() && doc.object().contains(QStringLiteral("error"))) {
        m_partitionTable->setRowCount(0);
        return;
    }
    const QJsonArray swaps = doc.isArray() ? doc.array() : QJsonArray();
    populatePartitionTable(swaps);
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
    updateActionEnabled();
}

void SwapfileWidget::populatePartitionTable(const QJsonArray &swaps) {
    QJsonArray partitions;
    for (const QJsonValue &value : swaps) {
        const QJsonObject entry = value.toObject();
        const QString type = JsonLoader::optionalString(entry, QStringLiteral("swap_type"));
        const QString name = JsonLoader::optionalString(entry, QStringLiteral("name"));
        if (type == QLatin1String("file")) {
            continue;
        }
        if (name.contains(QStringLiteral("zram"), Qt::CaseInsensitive)) {
            continue;
        }
        partitions.append(entry);
    }

    m_partitionTable->setRowCount(partitions.size());
    int row = 0;
    for (const QJsonValue &value : partitions) {
        const QJsonObject entry = value.toObject();
        const QString name = JsonLoader::optionalString(entry, QStringLiteral("name"));
        const bool active = JsonLoader::optionalBool(entry, QStringLiteral("active"), true);
        auto *nameItem = new QTableWidgetItem(name);
        nameItem->setData(Qt::UserRole, name);
        m_partitionTable->setItem(row, 0, nameItem);
        m_partitionTable->setItem(row, 1,
                                  new QTableWidgetItem(active ? tr("active") : tr("inactive")));
        m_partitionTable->setItem(row, 2,
                                  new QTableWidgetItem(QString::number(
                                      JsonLoader::optionalInt(entry, QStringLiteral("priority")))));
        ++row;
    }
    updateActionEnabled();
}

QString SwapfileWidget::selectedPath() const {
    const auto rows = m_table->selectionModel()->selectedRows();
    if (rows.isEmpty()) {
        return {};
    }
    const QTableWidgetItem *item = m_table->item(rows.first().row(), 0);
    return item ? item->text() : QString();
}

QString SwapfileWidget::selectedPartitionDevice() const {
    const auto rows = m_partitionTable->selectionModel()->selectedRows();
    if (rows.isEmpty()) {
        return {};
    }
    const QTableWidgetItem *item = m_partitionTable->item(rows.first().row(), 0);
    return item ? item->data(Qt::UserRole).toString() : QString();
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

void SwapfileWidget::stageCreate() {
    if (m_pathEdit->text().isEmpty()) {
        return;
    }
    if (m_onBtrfs) {
        const QString check = XzramCli::swapfileCheckJson(m_pathEdit->text());
        QString parseError;
        const QJsonObject root = JsonLoader::parseObject(check, &parseError);
        if (!JsonLoader::optionalBool(root, QStringLiteral("ready"), false)) {
            const auto answer = QMessageBox::question(
                this, tr("Btrfs not ready"),
                tr("This path is not ready for a swap file yet. Prepare the directory first?\n\n%1")
                    .arg(JsonLoader::optionalString(root, QStringLiteral("message"))));
            if (answer == QMessageBox::Yes) {
                prepareBtrfs();
            }
            return;
        }
    }
    QString error;
    if (!XzramCli::swapfileCreate(m_pathEdit->text(), static_cast<quint64>(m_sizeSpin->value()),
                                  m_prioritySpin->value(), &error)) {
        QMessageBox::warning(this, tr("Stage failed"), error);
        return;
    }
    captureCreateBaseline();
    updateActionEnabled();
    emit stagingChanged();
}

void SwapfileWidget::stageResize() {
    const QString path = selectedPath();
    if (path.isEmpty()) {
        QMessageBox::information(this, tr("Resize"), tr("Select a swap file row first."));
        return;
    }
    QString error;
    if (!XzramCli::swapfileResize(path, static_cast<quint64>(m_sizeSpin->value()), &error)) {
        QMessageBox::warning(this, tr("Stage failed"), error);
        return;
    }
    emit stagingChanged();
}

void SwapfileWidget::stageRemove() {
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
    if (!XzramCli::swapfileRemove(path, &error)) {
        QMessageBox::warning(this, tr("Stage failed"), error);
        return;
    }
    emit stagingChanged();
}

void SwapfileWidget::swapOnSelected() {
    const QString device = selectedPartitionDevice();
    if (device.isEmpty()) {
        QMessageBox::information(this, tr("Enable swap"), tr("Select a swap partition first."));
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
    QString error;
    if (!XzramCli::swapOff(device, &error)) {
        QMessageBox::warning(this, tr("Disable swap failed"), error);
        return;
    }
    emit refreshRequested();
}
