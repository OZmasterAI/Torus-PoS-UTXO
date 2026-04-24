#ifndef STAKEPAGE_H
#define STAKEPAGE_H

#include <QWidget>

namespace Ui {
    class StakePage;
}
class WalletModel;

class StakePage : public QWidget
{
    Q_OBJECT

public:
    explicit StakePage(QWidget *parent = 0);
    ~StakePage();

    void setModel(WalletModel *model);

public slots:
    void setBalance(qint64 balance, qint64 stake, qint64 unconfirmedBalance, qint64 immatureBalance, qint64 permanentStakeBalance);

private slots:
    void on_stakeButton_clicked();
    void updateDisplayUnit();

private:
    Ui::StakePage *ui;
    WalletModel *model;
};

#endif // STAKEPAGE_H
