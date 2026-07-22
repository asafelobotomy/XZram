#include "snapshotwidget.h"

#include "jsonloader.h"
#include "xzramcli.h"

#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QSpinBox>
#include <QTableWidget>
#include <QVBoxLayout>

SnapshotWidget::SnapshotWidget(QWidget *parent) : QWidget(parent) {
    setupUi();
}

void SnapshotWidget::setupUi() {
    auto *layout = new QVBoxLayout(this);

    auto *heading = new QLabel(tr("<b>Snapshots & rollback</b>"), this);
    layout->addWidget(heading);

    m_noteLabel = new QLabel(
        tr("Automatic snapshots are taken when the app opens and before every apply. "
           "Create, restore, delete, or prune snapshots here. Rollback restores the last "
           "known-good configuration."),
        this);
    m_noteLabel->setWordWrap(true);
    layout->addWidget(m_noteLabel);

    m_table = new QTableWidget(0, 4, this);
    m_table->setHorizontalHeaderLabels(
        {tr("Label"), tr("Date"), tr("Trigger"), tr("Summary")});
    m_table->horizontalHeader()->setStretchLastSection(true);
    m_table->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_table->setSelectionMode(QAbstractItemView::SingleSelection);
    m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
    layout->addWidget(m_table, 1);

    auto *createRow = new QHBoxLayout();
    m_labelEdit = new QLineEdit(this);
    m_labelEdit->setPlaceholderText(tr("Optional label"));
    m_createButton = new QPushButton(tr("Create snapshot"), this);
    m_createButton->setToolTip(
        tr("Save a copy of the current swap configuration you can restore later."));
    createRow->addWidget(m_labelEdit, 1);
    createRow->addWidget(m_createButton);
    layout->addLayout(createRow);

    auto *actions = new QHBoxLayout();
    m_restoreButton = new QPushButton(tr("Restore selected"), this);
    m_deleteButton = new QPushButton(tr("Delete selected"), this);
    m_restoreButton->setToolTip(
        tr("Put the selected snapshot’s settings back into effect on this system."));
    m_deleteButton->setToolTip(tr("Permanently remove the selected snapshot."));
    m_restoreButton->setEnabled(false);
    m_deleteButton->setEnabled(false);
    actions->addWidget(m_restoreButton);
    actions->addWidget(m_deleteButton);
    actions->addStretch();
    layout->addLayout(actions);

    auto *pruneRow = new QHBoxLayout();
    m_pruneKeepSpin = new QSpinBox(this);
    m_pruneKeepSpin->setRange(1, 1000);
    m_pruneKeepSpin->setValue(50);
    m_pruneKeepSpin->setPrefix(tr("Keep "));
    m_pruneButton = new QPushButton(tr("Delete older snapshots"), this);
    m_pruneButton->setToolTip(
        tr("Keep the newest snapshots (count on the left) and delete the rest."));
    pruneRow->addWidget(m_pruneKeepSpin);
    pruneRow->addWidget(m_pruneButton);
    pruneRow->addStretch();
    layout->addLayout(pruneRow);

    m_rollbackButton = new QPushButton(tr("Restore last known-good"), this);
    m_rollbackButton->setToolTip(
        tr("Immediately restore the last known-good configuration saved before an apply."));
    layout->addWidget(m_rollbackButton);

    connect(m_createButton, &QPushButton::clicked, this, &SnapshotWidget::createSnapshot);
    connect(m_restoreButton, &QPushButton::clicked, this, &SnapshotWidget::restoreSelected);
    connect(m_deleteButton, &QPushButton::clicked, this, &SnapshotWidget::deleteSelected);
    connect(m_pruneButton, &QPushButton::clicked, this, &SnapshotWidget::pruneSnapshots);
    connect(m_rollbackButton, &QPushButton::clicked, this, &SnapshotWidget::rollback);
    connect(m_table, &QTableWidget::itemSelectionChanged, this, [this]() {
        const bool selected = m_table->currentRow() >= 0;
        m_restoreButton->setEnabled(selected);
        m_deleteButton->setEnabled(selected);
    });
}

void SnapshotWidget::setPruneKeepDefault(int keep) {
    if (keep >= m_pruneKeepSpin->minimum() && keep <= m_pruneKeepSpin->maximum()) {
        m_pruneKeepSpin->setValue(keep);
    }
}

QString SnapshotWidget::selectedSnapshotId() const {
    const int row = m_table->currentRow();
    if (row < 0) {
        return {};
    }
    const QTableWidgetItem *labelItem = m_table->item(row, 0);
    return labelItem ? labelItem->data(Qt::UserRole).toString() : QString();
}

void SnapshotWidget::refresh() {
    QString parseError;
    const QJsonArray snapshots =
        JsonLoader::parseArray(XzramCli::snapshotsJson(), &parseError);

    m_table->setRowCount(0);
    m_restoreButton->setEnabled(false);
    m_deleteButton->setEnabled(false);
    if (!parseError.isEmpty() || snapshots.isEmpty()) {
        return;
    }

    m_table->setRowCount(snapshots.size());
    for (int row = 0; row < snapshots.size(); ++row) {
        const QJsonObject snap = snapshots.at(row).toObject();
        const QString id = snap.value(QStringLiteral("id")).toString();
        const QString label = snap.value(QStringLiteral("label")).toString();
        const QString created = snap.value(QStringLiteral("created_at")).toString();
        const QString trigger = snap.value(QStringLiteral("trigger")).toString();
        const QString summary = snap.value(QStringLiteral("pending_summary")).toString();

        auto *labelItem = new QTableWidgetItem(label);
        labelItem->setData(Qt::UserRole, id);
        m_table->setItem(row, 0, labelItem);
        m_table->setItem(row, 1, new QTableWidgetItem(created));
        m_table->setItem(row, 2, new QTableWidgetItem(trigger));
        m_table->setItem(row, 3, new QTableWidgetItem(summary));
    }
    m_table->resizeColumnsToContents();
}

void SnapshotWidget::createSnapshot() {
    QString error;
    if (!XzramCli::snapshotCreate(m_labelEdit->text().trimmed(), &error)) {
        QMessageBox::warning(this, tr("Create failed"), error);
        return;
    }
    m_labelEdit->clear();
    refresh();
}

void SnapshotWidget::restoreSelected() {
    const QString id = selectedSnapshotId();
    if (id.isEmpty()) {
        return;
    }
    const int row = m_table->currentRow();
    const QString label = m_table->item(row, 0) ? m_table->item(row, 0)->text() : id;

    const auto answer = QMessageBox::question(
        this, tr("Restore snapshot"),
        tr("Restore configuration from snapshot?\n\n%1\n\nID: %2").arg(label, id),
        QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
    if (answer != QMessageBox::Yes) {
        return;
    }

    QString error;
    if (!XzramCli::snapshotRestore(id, &error)) {
        QMessageBox::warning(this, tr("Restore failed"), error);
        return;
    }
    QMessageBox::information(this, tr("Restore complete"),
                             tr("Configuration restored from snapshot."));
    refresh();
    emit configurationChanged();
}

void SnapshotWidget::deleteSelected() {
    const QString id = selectedSnapshotId();
    if (id.isEmpty()) {
        return;
    }
    const int row = m_table->currentRow();
    const QString label = m_table->item(row, 0) ? m_table->item(row, 0)->text() : id;

    const auto answer = QMessageBox::question(
        this, tr("Delete snapshot"),
        tr("Permanently delete snapshot?\n\n%1\n\nID: %2").arg(label, id),
        QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
    if (answer != QMessageBox::Yes) {
        return;
    }

    QString error;
    if (!XzramCli::snapshotDelete(id, &error)) {
        QMessageBox::warning(this, tr("Delete failed"), error);
        return;
    }
    refresh();
}

void SnapshotWidget::pruneSnapshots() {
    const int keep = m_pruneKeepSpin->value();
    const auto answer = QMessageBox::question(
        this, tr("Prune snapshots"),
        tr("Delete older snapshots, keeping the newest %1?").arg(keep),
        QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
    if (answer != QMessageBox::Yes) {
        return;
    }

    QString error;
    if (!XzramCli::snapshotPrune(keep, &error)) {
        QMessageBox::warning(this, tr("Prune failed"), error);
        return;
    }
    refresh();
}

void SnapshotWidget::rollback() {
    const auto answer = QMessageBox::question(
        this, tr("Rollback"),
        tr("Restore the last known-good configuration? This applies immediately."),
        QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
    if (answer != QMessageBox::Yes) {
        return;
    }

    QString error;
    if (!XzramCli::rollback(&error)) {
        QMessageBox::warning(this, tr("Rollback failed"), error);
        return;
    }
    QMessageBox::information(this, tr("Rollback complete"),
                             tr("Last known-good configuration restored."));
    refresh();
    emit configurationChanged();
}
