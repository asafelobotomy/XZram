#ifndef REFRESHRUNNER_H
#define REFRESHRUNNER_H

#include <QHash>
#include <QObject>
#include <QString>
#include <QStringList>

class QProcess;

/// Runs a batch of read-only xzram CLI queries (plus `systemctl is-active xzramd`)
/// concurrently without blocking the UI thread, then emits `finished` once.
class RefreshRunner : public QObject {
    Q_OBJECT

public:
    enum class Query { Status, Detect, Doctor, Swaps, ZramConfig, Swapfiles, Sysctl, Pending,
                       Snapshots };

    struct Results {
        QHash<int, QString> json; // keyed by Query; JSON or {"error": "..."}
        bool daemonActive = false;

        QString value(Query q) const { return json.value(static_cast<int>(q)); }
        bool has(Query q) const { return json.contains(static_cast<int>(q)); }
    };

    explicit RefreshRunner(QObject *parent = nullptr);

    bool isRunning() const { return m_outstanding > 0; }
    /// Start the given queries. Returns false (and does nothing) if a batch is in flight.
    bool start(const QList<Query> &queries);

signals:
    void finished(const RefreshRunner::Results &results);

private:
    void startQuery(Query query);
    void startDaemonCheck();
    void completeOne();

    Results m_results;
    int m_outstanding = 0;
};

#endif
