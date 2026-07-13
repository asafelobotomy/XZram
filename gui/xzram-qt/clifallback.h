#ifndef CLIFALLBACK_H
#define CLIFALLBACK_H

#include <QString>

namespace CliFallback {
QString run(const QStringList &args, int timeoutMs = 10000);
QString statusJson();
QString detectionJson();
QString doctorJson();
QString zramConfigJson();
QString swapfilesJson();
QString swapfileCheckJson(const QString &path);
QString swapsJson();
QString sysctlJson();
QString pendingJson();
QString recommendedDefaultsJson();
}

#endif
