#include "pendingbanner.h"

#include <QHBoxLayout>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QPushButton>

namespace {

int countPendingChanges(const QJsonValue &pending) {
    if (pending.isNull() || !pending.isObject()) {
        return 0;
    }
    const QJsonObject obj = pending.toObject();
    int count = 0;
    if (!obj.value(QStringLiteral("zram")).isNull()) {
        ++count;
    }
    if (obj.value(QStringLiteral("disable_zram")).toBool()) {
        ++count;
    }
    if (!obj.value(QStringLiteral("swapfile")).isNull()) {
        ++count;
    }
    if (!obj.value(QStringLiteral("swapfile_resize")).isNull()) {
        ++count;
    }
    if (!obj.value(QStringLiteral("remove_swapfile")).isNull()) {
        ++count;
    }
    if (!obj.value(QStringLiteral("sysctl")).isNull()) {
        ++count;
    }
    return count;
}

} // namespace

PendingBanner::PendingBanner(QWidget *parent) : QWidget(parent) {
    auto *layout = new QHBoxLayout(this);
    layout->setContentsMargins(8, 6, 8, 6);

    m_label = new QLabel(this);
    layout->addWidget(m_label, 1);

    m_applyButton = new QPushButton(tr("Apply"), this);
    m_clearButton = new QPushButton(tr("Clear"), this);
    layout->addWidget(m_applyButton);
    layout->addWidget(m_clearButton);

    setStyleSheet(QStringLiteral(
        "PendingBanner { background-color: #fff3cd; border: 1px solid #ffc107; border-radius: 4px; }"));
    hide();

    connect(m_applyButton, &QPushButton::clicked, this, &PendingBanner::applyRequested);
    connect(m_clearButton, &QPushButton::clicked, this, &PendingBanner::clearRequested);
}

void PendingBanner::setDaemonAvailable(bool available) {
    m_applyButton->setEnabled(available);
    m_clearButton->setEnabled(available);
}

void PendingBanner::setPendingJson(const QString &json) {
    const QString trimmed = json.trimmed();
    if (trimmed.isEmpty() || trimmed == QLatin1String("null")) {
        hide();
        return;
    }

    QJsonParseError parseError;
    const QJsonDocument doc = QJsonDocument::fromJson(trimmed.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError) {
        hide();
        return;
    }

    if (doc.isObject() && doc.object().contains(QStringLiteral("error"))) {
        hide();
        return;
    }

    const QJsonValue pending = doc.isObject() ? QJsonValue(doc.object()) : QJsonValue();
    const int count = countPendingChanges(pending);
    if (count == 0) {
        hide();
        return;
    }

    m_label->setText(tr("%n staged change(s) — apply or clear to continue", "", count));
    show();
}
