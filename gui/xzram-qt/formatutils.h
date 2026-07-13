#ifndef FORMATUTILS_H
#define FORMATUTILS_H

#include <QString>

namespace FormatUtils {
QString formatBytes(quint64 bytes);
QString formatPercent(double ratio);
QString compressionRatio(quint64 dataBytes, quint64 compressedBytes);
QString humanizeEnum(const QString &value);
QString swapSourceLabel(const QString &source);
QString severityLabel(const QString &severity);
}

#endif
