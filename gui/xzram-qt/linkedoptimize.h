#ifndef LINKEDOPTIMIZE_H
#define LINKEDOPTIMIZE_H

#include <QJsonObject>
#include <QString>

class QLabel;
class SwapfileWidget;
class SysctlWidget;
class ZramWidget;

namespace LinkedOptimize {

QString gatherSeedJson(const ZramWidget *zram, const SysctlWidget *sysctl,
                       const SwapfileWidget *swapfile);
void applyResult(ZramWidget *zram, SysctlWidget *sysctl, SwapfileWidget *swapfile, QLabel *status,
                 const QJsonObject &result);

} // namespace LinkedOptimize

#endif
