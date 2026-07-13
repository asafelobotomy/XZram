#include "doctorwidget.h"

#include "dbusclient.h"
#include "formatutils.h"
#include "jsonloader.h"

#include <QClipboard>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QScrollArea>
#include <QVBoxLayout>

DoctorWidget::DoctorWidget(DbusClient *client, QWidget *parent)
    : QWidget(parent), m_client(client) {
    auto *outer = new QVBoxLayout(this);

    m_header = new QLabel(this);
    m_header->setAlignment(Qt::AlignCenter);
    m_header->setMinimumHeight(36);
    outer->addWidget(m_header);

    auto *scroll = new QScrollArea(this);
    scroll->setWidgetResizable(true);
    m_issuesContainer = new QWidget(scroll);
    m_issuesLayout = new QVBoxLayout(m_issuesContainer);
    m_issuesLayout->addStretch();
    scroll->setWidget(m_issuesContainer);
    outer->addWidget(scroll, 1);

    clearIssues();
}

void DoctorWidget::clearIssues() {
    m_header->setText(tr("No diagnostics yet"));
    m_header->setStyleSheet(
        QStringLiteral("background: #e9ecef; border-radius: 4px; padding: 6px;"));

    while (QLayoutItem *item = m_issuesLayout->takeAt(0)) {
        if (QWidget *widget = item->widget()) {
            widget->deleteLater();
        }
        delete item;
    }
    m_issuesLayout->addStretch();
}

QWidget *DoctorWidget::makeIssueCard(const QJsonObject &issue) {
    const QString severity = JsonLoader::optionalString(issue, QStringLiteral("severity"));
    const QString message = JsonLoader::optionalString(issue, QStringLiteral("message"));
    const QString suggestion = JsonLoader::optionalString(issue, QStringLiteral("suggestion"));
    const QJsonObject action = issue.value(QStringLiteral("action")).toObject();

    auto *card = new QWidget(m_issuesContainer);
    auto *layout = new QVBoxLayout(card);
    layout->setContentsMargins(10, 8, 10, 8);

    QString bg = QStringLiteral("#d1ecf1");
    QString fg = QStringLiteral("#0c5460");
    if (severity == QLatin1String("warning")) {
        bg = QStringLiteral("#fff3cd");
        fg = QStringLiteral("#856404");
    } else if (severity == QLatin1String("error")) {
        bg = QStringLiteral("#f8d7da");
        fg = QStringLiteral("#721c24");
    }
    card->setStyleSheet(QStringLiteral("background: %1; color: %2; border-radius: 6px;").arg(bg, fg));

    auto *title = new QLabel(
        QStringLiteral("<b>%1</b> — %2").arg(FormatUtils::severityLabel(severity), message), card);
    title->setWordWrap(true);
    layout->addWidget(title);

    if (!suggestion.isEmpty()) {
        auto *suggestRow = new QHBoxLayout();
        auto *suggest = new QLabel(QStringLiteral("<i>%1</i>").arg(suggestion), card);
        suggest->setWordWrap(true);
        suggestRow->addWidget(suggest, 1);

        auto *copyButton = new QPushButton(tr("Copy suggestion"), card);
        connect(copyButton, &QPushButton::clicked, card, [suggestion]() {
            QGuiApplication::clipboard()->setText(suggestion);
        });
        suggestRow->addWidget(copyButton);
        layout->addLayout(suggestRow);
    }

    const QString actionType = JsonLoader::optionalString(action, QStringLiteral("type"));
    if (actionType == QLatin1String("prepare_btrfs_swapfile")) {
        const QString path = JsonLoader::optionalString(action, QStringLiteral("path"));
        auto *prepareButton = new QPushButton(tr("Prepare nodatacow for %1").arg(path), card);
        connect(prepareButton, &QPushButton::clicked, this, [this, path]() {
            QString error;
            if (!m_client->prepareSwapfileBtrfs(path, true, &error)) {
                QMessageBox::warning(this, tr("Prepare failed"), error);
                return;
            }
            QMessageBox::information(this, tr("Prepare complete"),
                                     tr("Nodatacow applied for %1").arg(path));
            emit btrfsPrepared();
        });
        layout->addWidget(prepareButton);
    }

    return card;
}

void DoctorWidget::setDoctorJson(const QString &json) {
    clearIssues();

    QString error;
    const QJsonObject root = JsonLoader::parseObject(json, &error);
    if (root.contains(QStringLiteral("error"))) {
        m_header->setText(root.value(QStringLiteral("error")).toString());
        m_header->setStyleSheet(
            QStringLiteral("background: #f8d7da; color: #721c24; border-radius: 4px; padding: 6px;"));
        return;
    }

    const bool healthy = JsonLoader::optionalBool(root, QStringLiteral("healthy"), true);
    const QJsonArray issues = root.value(QStringLiteral("issues")).toArray();

    if (healthy) {
        m_header->setText(tr("System healthy"));
        m_header->setStyleSheet(
            QStringLiteral("background: #d4edda; color: #155724; border-radius: 4px; padding: 6px;"));
    } else {
        m_header->setText(tr("%1 issue(s) found").arg(issues.size()));
        m_header->setStyleSheet(
            QStringLiteral("background: #fff3cd; color: #856404; border-radius: 4px; padding: 6px;"));
    }

    m_issuesLayout->takeAt(m_issuesLayout->count() - 1);
    if (issues.isEmpty() && healthy) {
        auto *ok = new QLabel(tr("No issues detected."), m_issuesContainer);
        m_issuesLayout->addWidget(ok);
    } else {
        for (const QJsonValue &value : issues) {
            m_issuesLayout->addWidget(makeIssueCard(value.toObject()));
        }
    }
    m_issuesLayout->addStretch();
}
