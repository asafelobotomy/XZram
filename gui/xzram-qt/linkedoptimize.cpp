#include "linkedoptimize.h"

#include "widgets/swapfilewidget.h"
#include "widgets/sysctlwidget.h"
#include "widgets/zramwidget.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QObject>

namespace LinkedOptimize {

QString gatherSeedJson(const ZramWidget *zram, const SysctlWidget *sysctl,
                       const SwapfileWidget *swapfile) {
    QJsonObject seed;
    seed.insert(QStringLiteral("zram"), zram->pendingSeedFragment());
    seed.insert(QStringLiteral("disable_zram"), false);
    const QJsonObject swap = swapfile->pendingSeedFragment();
    if (swap.isEmpty()) {
        seed.insert(QStringLiteral("swapfile"), QJsonValue());
    } else {
        seed.insert(QStringLiteral("swapfile"), swap);
    }
    seed.insert(QStringLiteral("swapfile_resize"), QJsonValue());
    seed.insert(QStringLiteral("remove_swapfile"), QJsonValue());
    seed.insert(QStringLiteral("sysctl"), sysctl->pendingSeedFragment());
    seed.insert(QStringLiteral("finalize_zram_tools"), false);
    return QString::fromUtf8(QJsonDocument(seed).toJson(QJsonDocument::Compact));
}

void applyResult(ZramWidget *zram, SysctlWidget *sysctl, SwapfileWidget *swapfile, QLabel *status,
                 const QJsonObject &result) {
    const QJsonObject pending = result.value(QStringLiteral("pending")).toObject();
    zram->setLinkedOptimizeBlocked(true);
    sysctl->setLinkedOptimizeBlocked(true);
    swapfile->setLinkedOptimizeBlocked(true);

    if (pending.contains(QStringLiteral("zram")) && !pending.value(QStringLiteral("zram")).isNull()) {
        zram->applyLinkedZram(pending.value(QStringLiteral("zram")).toObject());
    }
    if (pending.contains(QStringLiteral("sysctl"))
        && !pending.value(QStringLiteral("sysctl")).isNull()) {
        sysctl->applyLinkedSysctl(pending.value(QStringLiteral("sysctl")).toObject());
    }
    if (pending.contains(QStringLiteral("swapfile"))
        && !pending.value(QStringLiteral("swapfile")).isNull()) {
        swapfile->applyLinkedSwapfile(pending.value(QStringLiteral("swapfile")).toObject());
    }

    zram->setLinkedOptimizeBlocked(false);
    sysctl->setLinkedOptimizeBlocked(false);
    swapfile->setLinkedOptimizeBlocked(false);

    const QJsonArray adjustments = result.value(QStringLiteral("adjustments")).toArray();
    if (!adjustments.isEmpty() && status) {
        QStringList lines;
        for (const QJsonValue &v : adjustments) {
            lines.append(v.toString());
        }
        status->setText(
            QObject::tr("Linked optimize: %1").arg(lines.join(QStringLiteral(" · "))));
        status->setToolTip(lines.join(QLatin1Char('\n')));
    }
}

} // namespace LinkedOptimize
