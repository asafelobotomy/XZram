#ifndef RECOMMENDEDDEFAULTSSCALE_H
#define RECOMMENDEDDEFAULTSSCALE_H

#include <QJsonObject>
#include <QString>
#include <QWidget>

class QLabel;
class QSlider;

/// Three-tick Low / Recommended / High sliders for ZRAM and overflow swap.
class RecommendedDefaultsScalePanel : public QWidget {
    Q_OBJECT

public:
    explicit RecommendedDefaultsScalePanel(const QJsonObject &sizeScales, QWidget *parent = nullptr);

    QString zramScale() const;
    QString swapScale() const;

signals:
    void scalesChanged();

private:
    void updatePreview(QSlider *slider, QLabel *preview, const QJsonObject &group, bool isZram);
    static int scaleToTick(const QString &selected);
    static QString tickToScale(int tick);

    QSlider *m_zramSlider = nullptr;
    QSlider *m_swapSlider = nullptr;
    QLabel *m_zramPreview = nullptr;
    QLabel *m_swapPreview = nullptr;
    QJsonObject m_zramGroup;
    QJsonObject m_swapGroup;
};

#endif
