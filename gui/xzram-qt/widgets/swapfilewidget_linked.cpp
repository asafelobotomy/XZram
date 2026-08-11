#include "swapfilewidget.h"

#include "jsonloader.h"

#include <QLineEdit>
#include <QSpinBox>
#include <climits>

void SwapfileWidget::onCreateFormEdited() {
    updateActionEnabled();
    if (!m_linkedOptimizeBlocked) {
        emit linkedFieldEdited(QStringLiteral("swapfile_create"));
    }
}

void SwapfileWidget::setLinkedOptimizeBlocked(bool blocked) {
    m_linkedOptimizeBlocked = blocked;
}

QJsonObject SwapfileWidget::pendingSeedFragment() const {
    QJsonObject swapfile;
    const QString path = m_pathEdit->text().trimmed();
    if (path.isEmpty()) {
        return {};
    }
    swapfile.insert(QStringLiteral("path"), path);
    swapfile.insert(QStringLiteral("size_mb"), static_cast<qint64>(m_sizeSpin->value()));
    swapfile.insert(QStringLiteral("priority"), m_prioritySpin->value());
    return swapfile;
}

void SwapfileWidget::applyLinkedSwapfile(const QJsonObject &swapfile) {
    if (swapfile.isEmpty()) {
        return;
    }
    m_linkedOptimizeBlocked = true;
    const QString path = JsonLoader::optionalString(swapfile, QStringLiteral("path"));
    if (!path.isEmpty()) {
        m_pathEdit->setText(path);
    }
    const quint64 sizeMb = JsonLoader::optionalUInt64(swapfile, QStringLiteral("size_mib"), 0);
    const quint64 sizeMbAlt = JsonLoader::optionalUInt64(swapfile, QStringLiteral("size_mb"), 0);
    const quint64 size = sizeMb > 0 ? sizeMb : sizeMbAlt;
    if (size > 0) {
        m_sizeSpin->setValue(static_cast<int>(qMin(size, static_cast<quint64>(INT_MAX))));
    }
    if (swapfile.contains(QStringLiteral("priority"))) {
        m_prioritySpin->setValue(JsonLoader::optionalInt(swapfile, QStringLiteral("priority"), 10));
    }
    m_linkedOptimizeBlocked = false;
    updateActionEnabled();
}
