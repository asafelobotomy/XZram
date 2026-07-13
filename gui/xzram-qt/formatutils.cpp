#include "formatutils.h"

#include <cmath>

namespace FormatUtils {

QString formatBytes(quint64 bytes) {
    static const char *units[] = {"B", "KiB", "MiB", "GiB", "TiB"};
    double size = static_cast<double>(bytes);
    int unit = 0;
    while (size >= 1024.0 && unit < 4) {
        size /= 1024.0;
        ++unit;
    }
    if (unit == 0) {
        return QStringLiteral("%1 B").arg(bytes);
    }
    return QStringLiteral("%1 %2").arg(QString::number(size, 'f', 1), units[unit]);
}

QString formatPercent(double ratio) {
    return QStringLiteral("%1%").arg(QString::number(ratio * 100.0, 'f', 1));
}

QString compressionRatio(quint64 dataBytes, quint64 compressedBytes) {
    if (compressedBytes == 0) {
        return QStringLiteral("—");
    }
    const double ratio = static_cast<double>(dataBytes) / static_cast<double>(compressedBytes);
    return QStringLiteral("%1x").arg(QString::number(ratio, 'f', 1));
}

QString humanizeEnum(const QString &value) {
    QString out = value;
    out.replace('_', ' ');
    if (!out.isEmpty()) {
        out[0] = out[0].toUpper();
    }
    return out;
}

QString swapSourceLabel(const QString &source) {
    if (source == QLatin1String("active")) {
        return QStringLiteral("Active");
    }
    if (source == QLatin1String("fstab")) {
        return QStringLiteral("Fstab");
    }
    return humanizeEnum(source);
}

QString severityLabel(const QString &severity) {
    if (severity == QLatin1String("error")) {
        return QStringLiteral("Error");
    }
    if (severity == QLatin1String("warning")) {
        return QStringLiteral("Warning");
    }
    return QStringLiteral("Info");
}

} // namespace FormatUtils
