#ifndef JSONLOADER_H
#define JSONLOADER_H

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

namespace JsonLoader {
QJsonObject parseObject(const QString &json, QString *error = nullptr);
QJsonArray parseArray(const QString &json, QString *error = nullptr);
QString optionalString(const QJsonObject &obj, const QString &key);
int optionalInt(const QJsonObject &obj, const QString &key, int defaultValue = 0);
quint64 optionalUInt64(const QJsonObject &obj, const QString &key, quint64 defaultValue = 0);
bool optionalBool(const QJsonObject &obj, const QString &key, bool defaultValue = false);
/// Doctor report warning count: `warnings` when present, else counted from `issues`.
int doctorWarnings(const QJsonObject &report, const QJsonArray &issues);
}

#endif
