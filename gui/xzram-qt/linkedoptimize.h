#ifndef LINKEDOPTIMIZE_H
#define LINKEDOPTIMIZE_H

#include <QJsonObject>
#include <QObject>
#include <QString>
#include <functional>

class CliJob;
class QLabel;
class SwapfileWidget;
class SysctlWidget;
class ZramWidget;

namespace LinkedOptimize {

QString gatherSeedJson(const ZramWidget *zram, const SysctlWidget *sysctl,
                       const SwapfileWidget *swapfile);
void applyResult(ZramWidget *zram, SysctlWidget *sysctl, SwapfileWidget *swapfile, QLabel *status,
                 const QJsonObject &result);

/// Async optimize-linked via CliJob (stdin seed); cancels prior in-flight work.
class Runner : public QObject {
    Q_OBJECT

public:
    explicit Runner(QObject *parent = nullptr);

    void cancel();
    void start(const QString &anchor, const QString &seed, ZramWidget *zram, SysctlWidget *sysctl,
               SwapfileWidget *swapfile, QLabel *status,
               const std::function<void(bool)> &setApplying,
               const std::function<void()> &maybeRerun);

private:
    CliJob *m_job = nullptr;
    quint64 m_gen = 0;
};

} // namespace LinkedOptimize

#endif
