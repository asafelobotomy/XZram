#include "utilitieswidget.h"

#include "clifallback.h"
#include "dbusclient.h"
#include "jsonloader.h"

#include <QHeaderView>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QTableWidget>
#include <QVBoxLayout>

UtilitiesWidget::UtilitiesWidget(DbusClient *client, QWidget *parent)
    : QWidget(parent), m_client(client) {
    setupUi();
}

void UtilitiesWidget::setupUi() {
    auto *layout = new QVBoxLayout(this);

    auto *heading = new QLabel(tr("<b>Restore Snapshots</b>"), this);
    layout->addWidget(heading);

    m_noteLabel = new QLabel(
        tr("Automatic snapshots are taken when the app opens and before every apply. "
           "Snapshots cannot be deleted from the GUI; use "
           "<code>xzram snapshot delete</code> from the CLI if needed."),
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

    m_restoreButton = new QPushButton(tr("Restore selected snapshot"), this);
    m_restoreButton->setEnabled(false);
    layout->addWidget(m_restoreButton);

    connect(m_restoreButton, &QPushButton::clicked, this, &UtilitiesWidget::restoreSelected);
    connect(m_table, &QTableWidget::itemSelectionChanged, this, [this]() {
        m_restoreButton->setEnabled(m_table->currentRow() >= 0);
    });
}

QString UtilitiesWidget::fetchSnapshotsJson() const {
    if (m_client->isRegistered()) {
        return m_client->listSnapshotsJson();
    }
    return CliFallback::run({QStringLiteral("snapshot"), QStringLiteral("list"),
                             QStringLiteral("--json")});
}

void UtilitiesWidget::refresh() {
    QString parseError;
    const QJsonArray snapshots =
        JsonLoader::parseArray(fetchSnapshotsJson(), &parseError);

    m_table->setRowCount(0);
    if (!parseError.isEmpty() || snapshots.isEmpty()) {
        m_restoreButton->setEnabled(false);
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

void UtilitiesWidget::restoreSelected() {
    const int row = m_table->currentRow();
    if (row < 0) {
        return;
    }
    const QTableWidgetItem *labelItem = m_table->item(row, 0);
    if (!labelItem) {
        return;
    }
    const QString id = labelItem->data(Qt::UserRole).toString();
    const QString label = labelItem->text();

    const auto answer = QMessageBox::question(
        this, tr("Restore snapshot"),
        tr("Restore configuration from snapshot?\n\n%1\n\nID: %2")
            .arg(label, id),
        QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
    if (answer != QMessageBox::Yes) {
        return;
    }

    QString error;
    if (!m_client->restoreSnapshot(id, &error)) {
        QMessageBox::warning(this, tr("Restore failed"), error);
        return;
    }
    QMessageBox::information(this, tr("Restore complete"),
                             tr("Configuration restored from snapshot."));
    refresh();
}
