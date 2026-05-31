#include "stakepage.h"
#include "ui_stakepage.h"

#include "walletmodel.h"
#include "optionsmodel.h"
#include "bitcoinunits.h"
#include "guiutil.h"
#include "init.h"
#include "base58.h"
#include "main.h"
#include "script.h"
#include "coincontrol.h"
#include "coincontroldialog.h"

#include <QMessageBox>
#include <QComboBox>

StakePage::StakePage(QWidget *parent) :
    QWidget(parent),
    ui(new Ui::StakePage),
    model(0)
{
    ui->setupUi(this);
    connect(ui->coinControlButton, SIGNAL(clicked()), this, SLOT(on_coinControlButton_clicked()));
}

StakePage::~StakePage()
{
    delete ui;
}

void StakePage::setModel(WalletModel *model)
{
    this->model = model;

    if(model && model->getOptionsModel())
    {
        setBalance(model->getBalance(), model->getStake(), model->getUnconfirmedBalance(), model->getImmatureBalance(), model->getPermanentStakeBalance());
        connect(model, SIGNAL(balanceChanged(qint64, qint64, qint64, qint64, qint64)), this, SLOT(setBalance(qint64, qint64, qint64, qint64, qint64)));
        connect(model, SIGNAL(balanceChanged(qint64, qint64, qint64, qint64, qint64)), this, SLOT(updateStakeDestinations()));
        connect(model->getOptionsModel(), SIGNAL(displayUnitChanged(int)), this, SLOT(updateDisplayUnit()));
    }

    updateDisplayUnit();
    updateStakeDestinations();
}

void StakePage::setBalance(qint64 balance, qint64 stake, qint64 unconfirmedBalance, qint64 immatureBalance, qint64 permanentStakeBalance)
{
    Q_UNUSED(stake);
    Q_UNUSED(unconfirmedBalance);
    Q_UNUSED(immatureBalance);

    int unit = model ? model->getOptionsModel()->getDisplayUnit() : BitcoinUnits::BTC;
    ui->labelCurrentStakeValue->setText(BitcoinUnits::formatWithUnit(unit, permanentStakeBalance));
    ui->labelAvailableValue->setText(BitcoinUnits::formatWithUnit(unit, balance));
}

void StakePage::updateDisplayUnit()
{
    if(model && model->getOptionsModel())
    {
        ui->stakeAmount->setDisplayUnit(model->getOptionsModel()->getDisplayUnit());
        setBalance(model->getBalance(), model->getStake(), model->getUnconfirmedBalance(), model->getImmatureBalance(), model->getPermanentStakeBalance());
    }
}

void StakePage::on_stakeButton_clicked()
{
    if(!model)
        return;

    if(!ui->stakeAmount->validate())
    {
        ui->stakeAmount->setValid(false);
        return;
    }

    qint64 nAmount = ui->stakeAmount->value();

    if(nAmount <= 0)
    {
        ui->stakeAmount->setValid(false);
        return;
    }

    if(nAmount < MIN_PERMANENT_STAKE)
    {
        QMessageBox::warning(this, tr("Permanent Stake"),
            tr("Minimum amount for permanent staking is %1.")
                .arg(BitcoinUnits::formatWithUnit(BitcoinUnits::BTC, MIN_PERMANENT_STAKE)),
            QMessageBox::Ok);
        return;
    }

    QMessageBox::StandardButton reply = QMessageBox::warning(this, tr("Permanent Stake"),
        tr("You are about to PERMANENTLY lock %1 for staking.\n\n"
           "This is IRREVERSIBLE. The locked coins can never be spent or unlocked.\n"
           "Staking rewards will be paid as spendable coins.\n\n"
           "Are you sure?").arg(BitcoinUnits::formatWithUnit(BitcoinUnits::BTC, nAmount)),
        QMessageBox::Yes | QMessageBox::No, QMessageBox::No);

    if(reply != QMessageBox::Yes)
        return;

    WalletModel::UnlockContext ctx(model->requestUnlock());
    if(!ctx.isValid())
        return;

    // Build permanent stake script — reuse existing key or generate new one
    CReserveKey reservekey(pwalletMain);
    CScript scriptPermanent;
    int idx = ui->stakeDestination->currentIndex();
    QByteArray existingScript = ui->stakeDestination->itemData(idx).toByteArray();

    if(existingScript.isEmpty())
    {
        CPubKey vchPubKey;
        if(!reservekey.GetReservedKey(vchPubKey))
        {
            QMessageBox::critical(this, tr("Permanent Stake"),
                tr("Error: Keypool ran out, please call keypoolrefill first."),
                QMessageBox::Ok);
            return;
        }
        scriptPermanent << OP_PERMANENT_LOCK << vchPubKey << OP_CHECKSIG;
    }
    else
    {
        scriptPermanent = CScript(existingScript.begin(), existingScript.end());
    }

    CWalletTx wtx;
    int64_t nFeeRequired;
    const CCoinControl *coinControl = CoinControlDialog::coinControl->HasSelected() ? CoinControlDialog::coinControl : NULL;
    if(!pwalletMain->CreateTransaction(scriptPermanent, nAmount, wtx, reservekey, nFeeRequired, coinControl))
    {
        std::string strError;
        if (nAmount + nFeeRequired > pwalletMain->GetBalance())
            strError = strprintf(_("Error: This transaction requires a transaction fee of at least %s because of its amount, complexity, or use of recently received funds  "), FormatMoney(nFeeRequired).c_str());
        else
            strError = _("Error: Transaction creation failed  ");
        QMessageBox::critical(this, tr("Permanent Stake"),
            tr("Error: %1").arg(QString::fromStdString(strError)),
            QMessageBox::Ok);
        return;
    }

    if(!pwalletMain->CommitTransaction(wtx, reservekey))
    {
        QMessageBox::critical(this, tr("Permanent Stake"),
            tr("Error: The transaction was rejected."),
            QMessageBox::Ok);
        return;
    }

    if(existingScript.isEmpty())
        reservekey.KeepKey();
    CoinControlDialog::coinControl->UnSelectAll();
    coinControlUpdateLabels();
    updateStakeDestinations();

    QMessageBox::information(this, tr("Permanent Stake"),
        tr("Successfully locked %1 for permanent staking.\n\nTransaction: %2")
            .arg(BitcoinUnits::formatWithUnit(BitcoinUnits::BTC, nAmount))
            .arg(QString::fromStdString(wtx.GetHash().GetHex())),
        QMessageBox::Ok);

    ui->stakeAmount->clear();
}

void StakePage::on_coinControlButton_clicked()
{
    CoinControlDialog dlg;
    dlg.setModel(model);
    dlg.exec();
    coinControlUpdateLabels();
}

void StakePage::coinControlUpdateLabels()
{
    if(!model)
        return;

    if(CoinControlDialog::coinControl->HasSelected())
    {
        std::vector<COutPoint> vOutpoints;
        CoinControlDialog::coinControl->ListSelected(vOutpoints);

        qint64 nTotal = 0;
        for (const COutPoint& out : vOutpoints)
            nTotal += pwalletMain->mapWallet[out.hash].vout[out.n].nValue;

        int unit = model->getOptionsModel()->getDisplayUnit();
        ui->labelCoinControlInfo->setText(
            tr("%1 inputs selected (%2)")
                .arg(vOutpoints.size())
                .arg(BitcoinUnits::formatWithUnit(unit, nTotal)));
        ui->labelCoinControlInfo->setStyleSheet("");
    }
    else
    {
        ui->labelCoinControlInfo->setText(tr("(automatic coin selection)"));
        ui->labelCoinControlInfo->setStyleSheet("QLabel { color: gray; }");
    }
}

void StakePage::updateStakeDestinations()
{
    if(!model)
        return;

    int prevIdx = ui->stakeDestination->currentIndex();
    QByteArray prevData = ui->stakeDestination->itemData(prevIdx).toByteArray();
    ui->stakeDestination->clear();
    ui->stakeDestination->addItem(tr("Create new address"), QByteArray());

    int unit = model->getOptionsModel()->getDisplayUnit();
    std::map<std::string, std::pair<int64_t, CScript> > mapStakes;

    {
        LOCK(pwalletMain->cs_wallet);
        for (std::map<uint256, CWalletTx>::const_iterator it = pwalletMain->mapWallet.begin();
             it != pwalletMain->mapWallet.end(); ++it)
        {
            const CWalletTx* pcoin = &(*it).second;
            if (!pcoin->IsFinal() || !pcoin->IsTrusted())
                continue;
            for (unsigned int i = 0; i < pcoin->vout.size(); i++)
            {
                if (pcoin->IsSpent(i) || !IsMine(*pwalletMain, pcoin->vout[i].scriptPubKey))
                    continue;
                if (!IsPermanentStakeScript(pcoin->vout[i].scriptPubKey))
                    continue;

                CTxDestination dest;
                if (!ExtractDestination(pcoin->vout[i].scriptPubKey, dest))
                    continue;

                std::string addr = CBitcoinAddress(dest).ToString();
                mapStakes[addr].first += pcoin->vout[i].nValue;
                mapStakes[addr].second = pcoin->vout[i].scriptPubKey;
            }
        }
    }

    int restoreIdx = 0;
    for (std::map<std::string, std::pair<int64_t, CScript> >::const_iterator it = mapStakes.begin();
         it != mapStakes.end(); ++it)
    {
        const CScript& script = it->second.second;
        QByteArray scriptData((const char*)&script[0], script.size());
        QString label = QString("%1 (%2)")
            .arg(QString::fromStdString(it->first))
            .arg(BitcoinUnits::formatWithUnit(unit, it->second.first));
        ui->stakeDestination->addItem(label, scriptData);

        if(!prevData.isEmpty() && scriptData == prevData)
            restoreIdx = ui->stakeDestination->count() - 1;
    }

    ui->stakeDestination->setCurrentIndex(restoreIdx);
}
