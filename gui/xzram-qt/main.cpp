#include "mainwindow.h"

#include <QApplication>
#include <QIcon>

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("xzram-qt"));
    QApplication::setApplicationDisplayName(QStringLiteral("XZram"));
    QApplication::setOrganizationName(QStringLiteral("XZram"));
    QApplication::setApplicationVersion(QStringLiteral(XZRAM_QT_VERSION));
    QApplication::setDesktopFileName(QStringLiteral("io.github.XZram"));
    QApplication::setWindowIcon(QIcon(QStringLiteral(":/icons/xzram-icon.png")));

    MainWindow window;
    window.show();
    return app.exec();
}
