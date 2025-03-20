//! *BIFF record* parsers
//!
//! Please refer to \[MS-XLS\] for documentation

pub mod from_record;
use super::{DataLocation, ExcelError, RecordStream, structures::*};
use ctxutils::io::*;
use from_record::*;
use from_record_derive::FromRecordStream;
use num_enum::FromPrimitive;
use std::{
    fmt,
    io::{Read, Seek},
};
use tracing::{debug, warn};

macro_rules! check_record_type {
    ($stream:expr, $expected_record_type:expr) => {
        if $stream.ty != $expected_record_type {
            return Err(format!(
                "Found unexpected stream type {:x?}; should be {:x?}",
                $stream.ty, $expected_record_type
            )
            .into());
        }
    };
}

pub(crate) fn load_dyn_record<R: Read + Seek>(
    stream: &mut RecordStream<R>,
) -> Result<Box<dyn Record>, ExcelError> {
    let mut anomalies = Anomalies::new();
    let r: Box<dyn Record> = match stream.ty {
        RecordType::Formula => Box::new(Formula::from_record(stream, &mut anomalies)?),
        RecordType::EOF => Box::new(EOF::from_record(stream, &mut anomalies)?),
        RecordType::CalcCount => Box::new(CalcCount::from_record(stream, &mut anomalies)?),
        RecordType::CalcMode => Box::new(CalcMode::from_record(stream, &mut anomalies)?),
        RecordType::CalcPrecision => Box::new(CalcPrecision::from_record(stream, &mut anomalies)?),
        RecordType::CalcRefMode => Box::new(CalcRefMode::from_record(stream, &mut anomalies)?),
        RecordType::CalcDelta => Box::new(CalcDelta::from_record(stream, &mut anomalies)?),
        RecordType::CalcIter => Box::new(CalcIter::from_record(stream, &mut anomalies)?),
        RecordType::Protect => Box::new(Protect::from_record(stream, &mut anomalies)?),
        RecordType::Password => Box::new(Password::from_record(stream, &mut anomalies)?),
        RecordType::Header => Box::new(Header::from_record(stream, &mut anomalies)?),
        RecordType::Footer => Box::new(Footer::from_record(stream, &mut anomalies)?),
        RecordType::ExternSheet => Box::new(ExternSheet::from_record(stream, &mut anomalies)?),
        RecordType::Lbl => Box::new(Lbl::from_record(stream, &mut anomalies)?),
        RecordType::WinProtect => Box::new(WinProtect::from_record(stream, &mut anomalies)?),
        RecordType::VerticalPageBreaks => {
            Box::new(VerticalPageBreaks::from_record(stream, &mut anomalies)?)
        }
        RecordType::HorizontalPageBreaks => {
            Box::new(HorizontalPageBreaks::from_record(stream, &mut anomalies)?)
        }
        RecordType::Note => Box::new(Note::from_record(stream, &mut anomalies)?),
        RecordType::Selection => Box::new(Selection::from_record(stream, &mut anomalies)?),
        RecordType::Date1904 => Box::new(Date1904::from_record(stream, &mut anomalies)?),
        RecordType::ExternName => Box::new(ExternName::from_record(stream, &mut anomalies)?),
        RecordType::LeftMargin => Box::new(LeftMargin::from_record(stream, &mut anomalies)?),
        RecordType::RightMargin => Box::new(RightMargin::from_record(stream, &mut anomalies)?),
        RecordType::TopMargin => Box::new(TopMargin::from_record(stream, &mut anomalies)?),
        RecordType::BottomMargin => Box::new(BottomMargin::from_record(stream, &mut anomalies)?),
        RecordType::PrintRowCol => Box::new(PrintRowCol::from_record(stream, &mut anomalies)?),
        RecordType::PrintGrid => Box::new(PrintGrid::from_record(stream, &mut anomalies)?),
        RecordType::FilePass => Box::new(FilePass::from_record(stream, &mut anomalies)?),
        RecordType::Font => Box::new(Font::from_record(stream, &mut anomalies)?),
        RecordType::PrintSize => Box::new(PrintSize::from_record(stream, &mut anomalies)?),
        RecordType::Continue => Box::new(Continue::from_record(stream, &mut anomalies)?),
        RecordType::Window1 => Box::new(Window1::from_record(stream, &mut anomalies)?),
        RecordType::Backup => Box::new(Backup::from_record(stream, &mut anomalies)?),
        RecordType::Pane => Box::new(Pane::from_record(stream, &mut anomalies)?),
        RecordType::CodePage => Box::new(CodePage::from_record(stream, &mut anomalies)?),
        RecordType::Pls => Box::new(Pls::from_record(stream, &mut anomalies)?),
        RecordType::DCon => Box::new(DCon::from_record(stream, &mut anomalies)?),
        RecordType::DConRef => Box::new(DConRef::from_record(stream, &mut anomalies)?),
        RecordType::DConName => Box::new(DConName::from_record(stream, &mut anomalies)?),
        RecordType::DefColWidth => Box::new(DefColWidth::from_record(stream, &mut anomalies)?),
        RecordType::XCT => Box::new(XCT::from_record(stream, &mut anomalies)?),
        RecordType::CRN => Box::new(CRN::from_record(stream, &mut anomalies)?),
        RecordType::FileSharing => Box::new(FileSharing::from_record(stream, &mut anomalies)?),
        RecordType::WriteAccess => Box::new(WriteAccess::from_record(stream, &mut anomalies)?),
        RecordType::Obj => Box::new(Obj::from_record(stream, &mut anomalies)?),
        RecordType::Uncalced => Box::new(Uncalced::from_record(stream, &mut anomalies)?),
        RecordType::CalcSaveRecalc => {
            Box::new(CalcSaveRecalc::from_record(stream, &mut anomalies)?)
        }
        RecordType::Template => Box::new(Template::from_record(stream, &mut anomalies)?),
        RecordType::Intl => Box::new(Intl::from_record(stream, &mut anomalies)?),
        RecordType::ObjProtect => Box::new(ObjProtect::from_record(stream, &mut anomalies)?),
        RecordType::ColInfo => Box::new(ColInfo::from_record(stream, &mut anomalies)?),
        RecordType::Guts => Box::new(Guts::from_record(stream, &mut anomalies)?),
        RecordType::WsBool => Box::new(WsBool::from_record(stream, &mut anomalies)?),
        RecordType::GridSet => Box::new(GridSet::from_record(stream, &mut anomalies)?),
        RecordType::HCenter => Box::new(HCenter::from_record(stream, &mut anomalies)?),
        RecordType::VCenter => Box::new(VCenter::from_record(stream, &mut anomalies)?),
        RecordType::BoundSheet8 => Box::new(BoundSheet8::from_record(stream, &mut anomalies)?),
        RecordType::WriteProtect => Box::new(WriteProtect::from_record(stream, &mut anomalies)?),
        RecordType::Country => Box::new(Country::from_record(stream, &mut anomalies)?),
        RecordType::HideObj => Box::new(HideObj::from_record(stream, &mut anomalies)?),
        RecordType::Sort => Box::new(Sort::from_record(stream, &mut anomalies)?),
        RecordType::Palette => Box::new(Palette::from_record(stream, &mut anomalies)?),
        RecordType::Sync => Box::new(Sync::from_record(stream, &mut anomalies)?),
        RecordType::LPr => Box::new(LPr::from_record(stream, &mut anomalies)?),
        RecordType::DxGCol => Box::new(DxGCol::from_record(stream, &mut anomalies)?),
        RecordType::FnGroupName => Box::new(FnGroupName::from_record(stream, &mut anomalies)?),
        RecordType::FilterMode => Box::new(FilterMode::from_record(stream, &mut anomalies)?),
        RecordType::BuiltInFnGroupCount => {
            Box::new(BuiltInFnGroupCount::from_record(stream, &mut anomalies)?)
        }
        RecordType::AutoFilterInfo => {
            Box::new(AutoFilterInfo::from_record(stream, &mut anomalies)?)
        }
        RecordType::AutoFilter => Box::new(AutoFilter::from_record(stream, &mut anomalies)?),
        RecordType::Scl => Box::new(Scl::from_record(stream, &mut anomalies)?),
        RecordType::Setup => Box::new(Setup::from_record(stream, &mut anomalies)?),
        RecordType::ScenMan => Box::new(ScenMan::from_record(stream, &mut anomalies)?),
        RecordType::SCENARIO => Box::new(SCENARIO::from_record(stream, &mut anomalies)?),
        RecordType::SxView => Box::new(SxView::from_record(stream, &mut anomalies)?),
        RecordType::Sxvd => Box::new(Sxvd::from_record(stream, &mut anomalies)?),
        RecordType::SXVI => Box::new(SXVI::from_record(stream, &mut anomalies)?),
        RecordType::SxIvd => Box::new(SxIvd::from_record(stream, &mut anomalies)?),
        RecordType::SXLI => Box::new(SXLI::from_record(stream, &mut anomalies)?),
        RecordType::SXPI => Box::new(SXPI::from_record(stream, &mut anomalies)?),
        RecordType::DocRoute => Box::new(DocRoute::from_record(stream, &mut anomalies)?),
        RecordType::RecipName => Box::new(RecipName::from_record(stream, &mut anomalies)?),
        RecordType::MulRk => Box::new(MulRk::from_record(stream, &mut anomalies)?),
        RecordType::MulBlank => Box::new(MulBlank::from_record(stream, &mut anomalies)?),
        RecordType::Mms => Box::new(Mms::from_record(stream, &mut anomalies)?),
        RecordType::SXDI => Box::new(SXDI::from_record(stream, &mut anomalies)?),
        RecordType::SXDB => Box::new(SXDB::from_record(stream, &mut anomalies)?),
        RecordType::SXFDB => Box::new(SXFDB::from_record(stream, &mut anomalies)?),
        RecordType::SXDBB => Box::new(SXDBB::from_record(stream, &mut anomalies)?),
        RecordType::SXNum => Box::new(SXNum::from_record(stream, &mut anomalies)?),
        RecordType::SxBool => Box::new(SxBool::from_record(stream, &mut anomalies)?),
        RecordType::SxErr => Box::new(SxErr::from_record(stream, &mut anomalies)?),
        RecordType::SXInt => Box::new(SXInt::from_record(stream, &mut anomalies)?),
        RecordType::SXString => Box::new(SXString::from_record(stream, &mut anomalies)?),
        RecordType::SXDtr => Box::new(SXDtr::from_record(stream, &mut anomalies)?),
        RecordType::SxNil => Box::new(SxNil::from_record(stream, &mut anomalies)?),
        RecordType::SXTbl => Box::new(SXTbl::from_record(stream, &mut anomalies)?),
        RecordType::SXTBRGIITM => Box::new(SXTBRGIITM::from_record(stream, &mut anomalies)?),
        RecordType::SxTbpg => Box::new(SxTbpg::from_record(stream, &mut anomalies)?),
        RecordType::ObProj => Box::new(ObProj::from_record(stream, &mut anomalies)?),
        RecordType::SXStreamID => Box::new(SXStreamID::from_record(stream, &mut anomalies)?),
        RecordType::DBCell => Box::new(DBCell::from_record(stream, &mut anomalies)?),
        RecordType::SXRng => Box::new(SXRng::from_record(stream, &mut anomalies)?),
        RecordType::SxIsxoper => Box::new(SxIsxoper::from_record(stream, &mut anomalies)?),
        RecordType::BookBool => Box::new(BookBool::from_record(stream, &mut anomalies)?),
        RecordType::DbOrParamQry => Box::new(DbOrParamQry::from_record(stream, &mut anomalies)?),
        RecordType::ScenarioProtect => {
            Box::new(ScenarioProtect::from_record(stream, &mut anomalies)?)
        }
        RecordType::OleObjectSize => Box::new(OleObjectSize::from_record(stream, &mut anomalies)?),
        RecordType::XF => Box::new(XF::from_record(stream, &mut anomalies)?),
        RecordType::InterfaceHdr => Box::new(InterfaceHdr::from_record(stream, &mut anomalies)?),
        RecordType::InterfaceEnd => Box::new(InterfaceEnd::from_record(stream, &mut anomalies)?),
        RecordType::SXVS => Box::new(SXVS::from_record(stream, &mut anomalies)?),
        RecordType::MergeCells => Box::new(MergeCells::from_record(stream, &mut anomalies)?),
        RecordType::BkHim => Box::new(BkHim::from_record(stream, &mut anomalies)?),
        RecordType::MsoDrawingGroup => {
            Box::new(MsoDrawingGroup::from_record(stream, &mut anomalies)?)
        }
        RecordType::MsoDrawing => Box::new(MsoDrawing::from_record(stream, &mut anomalies)?),
        RecordType::MsoDrawingSelection => {
            Box::new(MsoDrawingSelection::from_record(stream, &mut anomalies)?)
        }
        RecordType::PhoneticInfo => Box::new(PhoneticInfo::from_record(stream, &mut anomalies)?),
        RecordType::SxRule => Box::new(SxRule::from_record(stream, &mut anomalies)?),
        RecordType::SXEx => Box::new(SXEx::from_record(stream, &mut anomalies)?),
        RecordType::SxFilt => Box::new(SxFilt::from_record(stream, &mut anomalies)?),
        RecordType::SxDXF => Box::new(SxDXF::from_record(stream, &mut anomalies)?),
        RecordType::SxItm => Box::new(SxItm::from_record(stream, &mut anomalies)?),
        RecordType::SxName => Box::new(SxName::from_record(stream, &mut anomalies)?),
        RecordType::SxSelect => Box::new(SxSelect::from_record(stream, &mut anomalies)?),
        RecordType::SXPair => Box::new(SXPair::from_record(stream, &mut anomalies)?),
        RecordType::SxFmla => Box::new(SxFmla::from_record(stream, &mut anomalies)?),
        RecordType::SxFormat => Box::new(SxFormat::from_record(stream, &mut anomalies)?),
        RecordType::SST => Box::new(SST::from_record(stream, &mut anomalies)?),
        RecordType::LabelSst => Box::new(LabelSst::from_record(stream, &mut anomalies)?),
        RecordType::ExtSST => Box::new(ExtSST::from_record(stream, &mut anomalies)?),
        RecordType::SXVDEx => Box::new(SXVDEx::from_record(stream, &mut anomalies)?),
        RecordType::SXFormula => Box::new(SXFormula::from_record(stream, &mut anomalies)?),
        RecordType::SXDBEx => Box::new(SXDBEx::from_record(stream, &mut anomalies)?),
        RecordType::RRDInsDel => Box::new(RRDInsDel::from_record(stream, &mut anomalies)?),
        RecordType::RRDHead => Box::new(RRDHead::from_record(stream, &mut anomalies)?),
        RecordType::RRDChgCell => Box::new(RRDChgCell::from_record(stream, &mut anomalies)?),
        RecordType::RRTabId => Box::new(RRTabId::from_record(stream, &mut anomalies)?),
        RecordType::RRDRenSheet => Box::new(RRDRenSheet::from_record(stream, &mut anomalies)?),
        RecordType::RRSort => Box::new(RRSort::from_record(stream, &mut anomalies)?),
        RecordType::RRDMove => Box::new(RRDMove::from_record(stream, &mut anomalies)?),
        RecordType::RRFormat => Box::new(RRFormat::from_record(stream, &mut anomalies)?),
        RecordType::RRAutoFmt => Box::new(RRAutoFmt::from_record(stream, &mut anomalies)?),
        RecordType::RRInsertSh => Box::new(RRInsertSh::from_record(stream, &mut anomalies)?),
        RecordType::RRDMoveBegin => Box::new(RRDMoveBegin::from_record(stream, &mut anomalies)?),
        RecordType::RRDMoveEnd => Box::new(RRDMoveEnd::from_record(stream, &mut anomalies)?),
        RecordType::RRDInsDelBegin => {
            Box::new(RRDInsDelBegin::from_record(stream, &mut anomalies)?)
        }
        RecordType::RRDInsDelEnd => Box::new(RRDInsDelEnd::from_record(stream, &mut anomalies)?),
        RecordType::RRDConflict => Box::new(RRDConflict::from_record(stream, &mut anomalies)?),
        RecordType::RRDDefName => Box::new(RRDDefName::from_record(stream, &mut anomalies)?),
        RecordType::RRDRstEtxp => Box::new(RRDRstEtxp::from_record(stream, &mut anomalies)?),
        RecordType::LRng => Box::new(LRng::from_record(stream, &mut anomalies)?),
        RecordType::UsesELFs => Box::new(UsesELFs::from_record(stream, &mut anomalies)?),
        RecordType::DSF => Box::new(DSF::from_record(stream, &mut anomalies)?),
        RecordType::CUsr => Box::new(CUsr::from_record(stream, &mut anomalies)?),
        RecordType::CbUsr => Box::new(CbUsr::from_record(stream, &mut anomalies)?),
        RecordType::UsrInfo => Box::new(UsrInfo::from_record(stream, &mut anomalies)?),
        RecordType::UsrExcl => Box::new(UsrExcl::from_record(stream, &mut anomalies)?),
        RecordType::FileLock => Box::new(FileLock::from_record(stream, &mut anomalies)?),
        RecordType::RRDInfo => Box::new(RRDInfo::from_record(stream, &mut anomalies)?),
        RecordType::BCUsrs => Box::new(BCUsrs::from_record(stream, &mut anomalies)?),
        RecordType::UsrChk => Box::new(UsrChk::from_record(stream, &mut anomalies)?),
        RecordType::UserBView => Box::new(UserBView::from_record(stream, &mut anomalies)?),
        RecordType::UserSViewBegin => {
            Box::new(UserSViewBegin::from_record(stream, &mut anomalies)?)
        }
        RecordType::UserSViewEnd => Box::new(UserSViewEnd::from_record(stream, &mut anomalies)?),
        RecordType::RRDUserView => Box::new(RRDUserView::from_record(stream, &mut anomalies)?),
        RecordType::Qsi => Box::new(Qsi::from_record(stream, &mut anomalies)?),
        RecordType::SupBook => Box::new(SupBook::from_record(stream, &mut anomalies)?),
        RecordType::Prot4Rev => Box::new(Prot4Rev::from_record(stream, &mut anomalies)?),
        RecordType::CondFmt => Box::new(CondFmt::from_record(stream, &mut anomalies)?),
        RecordType::CF => Box::new(CF::from_record(stream, &mut anomalies)?),
        RecordType::DVal => Box::new(DVal::from_record(stream, &mut anomalies)?),
        RecordType::DConBin => Box::new(DConBin::from_record(stream, &mut anomalies)?),
        RecordType::TxO => Box::new(TxO::from_record(stream, &mut anomalies)?),
        RecordType::RefreshAll => Box::new(RefreshAll::from_record(stream, &mut anomalies)?),
        RecordType::HLink => Box::new(HLink::from_record(stream, &mut anomalies)?),
        RecordType::Lel => Box::new(Lel::from_record(stream, &mut anomalies)?),
        RecordType::CodeName => Box::new(CodeName::from_record(stream, &mut anomalies)?),
        RecordType::SXFDBType => Box::new(SXFDBType::from_record(stream, &mut anomalies)?),
        RecordType::Prot4RevPass => Box::new(Prot4RevPass::from_record(stream, &mut anomalies)?),
        RecordType::ObNoMacros => Box::new(ObNoMacros::from_record(stream, &mut anomalies)?),
        RecordType::Dv => Box::new(Dv::from_record(stream, &mut anomalies)?),
        RecordType::Excel9File => Box::new(Excel9File::from_record(stream, &mut anomalies)?),
        RecordType::RecalcId => Box::new(RecalcId::from_record(stream, &mut anomalies)?),
        RecordType::EntExU2 => Box::new(EntExU2::from_record(stream, &mut anomalies)?),
        RecordType::Dimensions => Box::new(Dimensions::from_record(stream, &mut anomalies)?),
        RecordType::Blank => Box::new(Blank::from_record(stream, &mut anomalies)?),
        RecordType::Number => Box::new(Number::from_record(stream, &mut anomalies)?),
        RecordType::Label => Box::new(Label::from_record(stream, &mut anomalies)?),
        RecordType::BoolErr => Box::new(BoolErr::from_record(stream, &mut anomalies)?),
        RecordType::String => Box::new(StringR::from_record(stream, &mut anomalies)?),
        RecordType::Row => Box::new(Row::from_record(stream, &mut anomalies)?),
        RecordType::Index => Box::new(Index::from_record(stream, &mut anomalies)?),
        RecordType::Array => Box::new(Array::from_record(stream, &mut anomalies)?),
        RecordType::DefaultRowHeight => {
            Box::new(DefaultRowHeight::from_record(stream, &mut anomalies)?)
        }
        RecordType::Table => Box::new(Table::from_record(stream, &mut anomalies)?),
        RecordType::Window2 => Box::new(Window2::from_record(stream, &mut anomalies)?),
        RecordType::RK => Box::new(RK::from_record(stream, &mut anomalies)?),
        RecordType::Style => Box::new(Style::from_record(stream, &mut anomalies)?),
        RecordType::BigName => Box::new(BigName::from_record(stream, &mut anomalies)?),
        RecordType::Format => Box::new(Format::from_record(stream, &mut anomalies)?),
        RecordType::ContinueBigName => {
            Box::new(ContinueBigName::from_record(stream, &mut anomalies)?)
        }
        RecordType::ShrFmla => Box::new(ShrFmla::from_record(stream, &mut anomalies)?),
        RecordType::HLinkTooltip => Box::new(HLinkTooltip::from_record(stream, &mut anomalies)?),
        RecordType::WebPub => Box::new(WebPub::from_record(stream, &mut anomalies)?),
        RecordType::QsiSXTag => Box::new(QsiSXTag::from_record(stream, &mut anomalies)?),
        RecordType::DBQueryExt => Box::new(DBQueryExt::from_record(stream, &mut anomalies)?),
        RecordType::ExtString => Box::new(ExtString::from_record(stream, &mut anomalies)?),
        RecordType::TxtQry => Box::new(TxtQry::from_record(stream, &mut anomalies)?),
        RecordType::Qsir => Box::new(Qsir::from_record(stream, &mut anomalies)?),
        RecordType::Qsif => Box::new(Qsif::from_record(stream, &mut anomalies)?),
        RecordType::RRDTQSIF => Box::new(RRDTQSIF::from_record(stream, &mut anomalies)?),
        RecordType::BOF => Box::new(Bof::from_record(stream, &mut anomalies)?),
        RecordType::OleDbConn => Box::new(OleDbConn::from_record(stream, &mut anomalies)?),
        RecordType::WOpt => Box::new(WOpt::from_record(stream, &mut anomalies)?),
        RecordType::SXViewEx => Box::new(SXViewEx::from_record(stream, &mut anomalies)?),
        RecordType::SXTH => Box::new(SXTH::from_record(stream, &mut anomalies)?),
        RecordType::SXPIEx => Box::new(SXPIEx::from_record(stream, &mut anomalies)?),
        RecordType::SXVDTEx => Box::new(SXVDTEx::from_record(stream, &mut anomalies)?),
        RecordType::SXViewEx9 => Box::new(SXViewEx9::from_record(stream, &mut anomalies)?),
        RecordType::ContinueFrt => Box::new(ContinueFrt::from_record(stream, &mut anomalies)?),
        RecordType::RealTimeData => Box::new(RealTimeData::from_record(stream, &mut anomalies)?),
        RecordType::ChartFrtInfo => Box::new(ChartFrtInfo::from_record(stream, &mut anomalies)?),
        RecordType::FrtWrapper => Box::new(FrtWrapper::from_record(stream, &mut anomalies)?),
        RecordType::StartBlock => Box::new(StartBlock::from_record(stream, &mut anomalies)?),
        RecordType::EndBlock => Box::new(EndBlock::from_record(stream, &mut anomalies)?),
        RecordType::StartObject => Box::new(StartObject::from_record(stream, &mut anomalies)?),
        RecordType::EndObject => Box::new(EndObject::from_record(stream, &mut anomalies)?),
        RecordType::CatLab => Box::new(CatLab::from_record(stream, &mut anomalies)?),
        RecordType::YMult => Box::new(YMult::from_record(stream, &mut anomalies)?),
        RecordType::SXViewLink => Box::new(SXViewLink::from_record(stream, &mut anomalies)?),
        RecordType::PivotChartBits => {
            Box::new(PivotChartBits::from_record(stream, &mut anomalies)?)
        }
        RecordType::FrtFontList => Box::new(FrtFontList::from_record(stream, &mut anomalies)?),
        RecordType::SheetExt => Box::new(SheetExt::from_record(stream, &mut anomalies)?),
        RecordType::BookExt => Box::new(BookExt::from_record(stream, &mut anomalies)?),
        RecordType::SXAddl => Box::new(SXAddl::from_record(stream, &mut anomalies)?),
        RecordType::CrErr => Box::new(CrErr::from_record(stream, &mut anomalies)?),
        RecordType::HFPicture => Box::new(HFPicture::from_record(stream, &mut anomalies)?),
        RecordType::FeatHdr => Box::new(FeatHdr::from_record(stream, &mut anomalies)?),
        RecordType::Feat => Box::new(Feat::from_record(stream, &mut anomalies)?),
        RecordType::DataLabExt => Box::new(DataLabExt::from_record(stream, &mut anomalies)?),
        RecordType::DataLabExtContents => {
            Box::new(DataLabExtContents::from_record(stream, &mut anomalies)?)
        }
        RecordType::CellWatch => Box::new(CellWatch::from_record(stream, &mut anomalies)?),
        RecordType::FeatHdr11 => Box::new(FeatHdr11::from_record(stream, &mut anomalies)?),
        RecordType::Feature11 => Box::new(Feature11::from_record(stream, &mut anomalies)?),
        RecordType::DropDownObjIds => {
            Box::new(DropDownObjIds::from_record(stream, &mut anomalies)?)
        }
        RecordType::ContinueFrt11 => Box::new(ContinueFrt11::from_record(stream, &mut anomalies)?),
        RecordType::DConn => Box::new(DConn::from_record(stream, &mut anomalies)?),
        RecordType::List12 => Box::new(List12::from_record(stream, &mut anomalies)?),
        RecordType::Feature12 => Box::new(Feature12::from_record(stream, &mut anomalies)?),
        RecordType::CondFmt12 => Box::new(CondFmt12::from_record(stream, &mut anomalies)?),
        RecordType::CF12 => Box::new(CF12::from_record(stream, &mut anomalies)?),
        RecordType::CFEx => Box::new(CFEx::from_record(stream, &mut anomalies)?),
        RecordType::XFCRC => Box::new(XFCRC::from_record(stream, &mut anomalies)?),
        RecordType::XFExt => Box::new(XFExt::from_record(stream, &mut anomalies)?),
        RecordType::AutoFilter12 => Box::new(AutoFilter12::from_record(stream, &mut anomalies)?),
        RecordType::ContinueFrt12 => Box::new(ContinueFrt12::from_record(stream, &mut anomalies)?),
        RecordType::MDTInfo => Box::new(MDTInfo::from_record(stream, &mut anomalies)?),
        RecordType::MDXStr => Box::new(MDXStr::from_record(stream, &mut anomalies)?),
        RecordType::MDXTuple => Box::new(MDXTuple::from_record(stream, &mut anomalies)?),
        RecordType::MDXSet => Box::new(MDXSet::from_record(stream, &mut anomalies)?),
        RecordType::MDXProp => Box::new(MDXProp::from_record(stream, &mut anomalies)?),
        RecordType::MDXKPI => Box::new(MDXKPI::from_record(stream, &mut anomalies)?),
        RecordType::MDB => Box::new(MDB::from_record(stream, &mut anomalies)?),
        RecordType::PLV => Box::new(PLV::from_record(stream, &mut anomalies)?),
        RecordType::Compat12 => Box::new(Compat12::from_record(stream, &mut anomalies)?),
        RecordType::DXF => Box::new(DXF::from_record(stream, &mut anomalies)?),
        RecordType::TableStyles => Box::new(TableStyles::from_record(stream, &mut anomalies)?),
        RecordType::TableStyle => Box::new(TableStyle::from_record(stream, &mut anomalies)?),
        RecordType::TableStyleElement => {
            Box::new(TableStyleElement::from_record(stream, &mut anomalies)?)
        }
        RecordType::StyleExt => Box::new(StyleExt::from_record(stream, &mut anomalies)?),
        RecordType::NamePublish => Box::new(NamePublish::from_record(stream, &mut anomalies)?),
        RecordType::NameCmt => Box::new(NameCmt::from_record(stream, &mut anomalies)?),
        RecordType::SortData => Box::new(SortData::from_record(stream, &mut anomalies)?),
        RecordType::Theme => Box::new(Theme::from_record(stream, &mut anomalies)?),
        RecordType::GUIDTypeLib => Box::new(GUIDTypeLib::from_record(stream, &mut anomalies)?),
        RecordType::FnGrp12 => Box::new(FnGrp12::from_record(stream, &mut anomalies)?),
        RecordType::NameFnGrp12 => Box::new(NameFnGrp12::from_record(stream, &mut anomalies)?),
        RecordType::MTRSettings => Box::new(MTRSettings::from_record(stream, &mut anomalies)?),
        RecordType::CompressPictures => {
            Box::new(CompressPictures::from_record(stream, &mut anomalies)?)
        }
        RecordType::HeaderFooter => Box::new(HeaderFooter::from_record(stream, &mut anomalies)?),
        RecordType::CrtLayout12 => Box::new(CrtLayout12::from_record(stream, &mut anomalies)?),
        RecordType::CrtMlFrt => Box::new(CrtMlFrt::from_record(stream, &mut anomalies)?),
        RecordType::CrtMlFrtContinue => {
            Box::new(CrtMlFrtContinue::from_record(stream, &mut anomalies)?)
        }
        RecordType::ForceFullCalculation => {
            Box::new(ForceFullCalculation::from_record(stream, &mut anomalies)?)
        }
        RecordType::ShapePropsStream => {
            Box::new(ShapePropsStream::from_record(stream, &mut anomalies)?)
        }
        RecordType::TextPropsStream => {
            Box::new(TextPropsStream::from_record(stream, &mut anomalies)?)
        }
        RecordType::RichTextStream => {
            Box::new(RichTextStream::from_record(stream, &mut anomalies)?)
        }
        RecordType::CrtLayout12A => Box::new(CrtLayout12A::from_record(stream, &mut anomalies)?),
        RecordType::Units => Box::new(Units::from_record(stream, &mut anomalies)?),
        RecordType::Chart => Box::new(Chart::from_record(stream, &mut anomalies)?),
        RecordType::Series => Box::new(Series::from_record(stream, &mut anomalies)?),
        RecordType::DataFormat => Box::new(DataFormat::from_record(stream, &mut anomalies)?),
        RecordType::LineFormat => Box::new(LineFormat::from_record(stream, &mut anomalies)?),
        RecordType::MarkerFormat => Box::new(MarkerFormat::from_record(stream, &mut anomalies)?),
        RecordType::AreaFormat => Box::new(AreaFormat::from_record(stream, &mut anomalies)?),
        RecordType::PieFormat => Box::new(PieFormat::from_record(stream, &mut anomalies)?),
        RecordType::AttachedLabel => Box::new(AttachedLabel::from_record(stream, &mut anomalies)?),
        RecordType::SeriesText => Box::new(SeriesText::from_record(stream, &mut anomalies)?),
        RecordType::ChartFormat => Box::new(ChartFormat::from_record(stream, &mut anomalies)?),
        RecordType::Legend => Box::new(Legend::from_record(stream, &mut anomalies)?),
        RecordType::SeriesList => Box::new(SeriesList::from_record(stream, &mut anomalies)?),
        RecordType::Bar => Box::new(Bar::from_record(stream, &mut anomalies)?),
        RecordType::Line => Box::new(Line::from_record(stream, &mut anomalies)?),
        RecordType::Pie => Box::new(Pie::from_record(stream, &mut anomalies)?),
        RecordType::Area => Box::new(Area::from_record(stream, &mut anomalies)?),
        RecordType::Scatter => Box::new(Scatter::from_record(stream, &mut anomalies)?),
        RecordType::CrtLine => Box::new(CrtLine::from_record(stream, &mut anomalies)?),
        RecordType::Axis => Box::new(Axis::from_record(stream, &mut anomalies)?),
        RecordType::Tick => Box::new(Tick::from_record(stream, &mut anomalies)?),
        RecordType::ValueRange => Box::new(ValueRange::from_record(stream, &mut anomalies)?),
        RecordType::CatSerRange => Box::new(CatSerRange::from_record(stream, &mut anomalies)?),
        RecordType::AxisLine => Box::new(AxisLine::from_record(stream, &mut anomalies)?),
        RecordType::CrtLink => Box::new(CrtLink::from_record(stream, &mut anomalies)?),
        RecordType::DefaultText => Box::new(DefaultText::from_record(stream, &mut anomalies)?),
        RecordType::Text => Box::new(Text::from_record(stream, &mut anomalies)?),
        RecordType::FontX => Box::new(FontX::from_record(stream, &mut anomalies)?),
        RecordType::ObjectLink => Box::new(ObjectLink::from_record(stream, &mut anomalies)?),
        RecordType::Frame => Box::new(Frame::from_record(stream, &mut anomalies)?),
        RecordType::Begin => Box::new(Begin::from_record(stream, &mut anomalies)?),
        RecordType::End => Box::new(End::from_record(stream, &mut anomalies)?),
        RecordType::PlotArea => Box::new(PlotArea::from_record(stream, &mut anomalies)?),
        RecordType::Chart3d => Box::new(Chart3d::from_record(stream, &mut anomalies)?),
        RecordType::PicF => Box::new(PicF::from_record(stream, &mut anomalies)?),
        RecordType::DropBar => Box::new(DropBar::from_record(stream, &mut anomalies)?),
        RecordType::Radar => Box::new(Radar::from_record(stream, &mut anomalies)?),
        RecordType::Surf => Box::new(Surf::from_record(stream, &mut anomalies)?),
        RecordType::RadarArea => Box::new(RadarArea::from_record(stream, &mut anomalies)?),
        RecordType::AxisParent => Box::new(AxisParent::from_record(stream, &mut anomalies)?),
        RecordType::LegendException => {
            Box::new(LegendException::from_record(stream, &mut anomalies)?)
        }
        RecordType::ShtProps => Box::new(ShtProps::from_record(stream, &mut anomalies)?),
        RecordType::SerToCrt => Box::new(SerToCrt::from_record(stream, &mut anomalies)?),
        RecordType::AxesUsed => Box::new(AxesUsed::from_record(stream, &mut anomalies)?),
        RecordType::SBaseRef => Box::new(SBaseRef::from_record(stream, &mut anomalies)?),
        RecordType::SerParent => Box::new(SerParent::from_record(stream, &mut anomalies)?),
        RecordType::SerAuxTrend => Box::new(SerAuxTrend::from_record(stream, &mut anomalies)?),
        RecordType::IFmtRecord => Box::new(IFmtRecord::from_record(stream, &mut anomalies)?),
        RecordType::Pos => Box::new(Pos::from_record(stream, &mut anomalies)?),
        RecordType::AlRuns => Box::new(AlRuns::from_record(stream, &mut anomalies)?),
        RecordType::BRAI => Box::new(BRAI::from_record(stream, &mut anomalies)?),
        RecordType::SerAuxErrBar => Box::new(SerAuxErrBar::from_record(stream, &mut anomalies)?),
        RecordType::ClrtClient => Box::new(ClrtClient::from_record(stream, &mut anomalies)?),
        RecordType::SerFmt => Box::new(SerFmt::from_record(stream, &mut anomalies)?),
        RecordType::Chart3DBarShape => {
            Box::new(Chart3DBarShape::from_record(stream, &mut anomalies)?)
        }
        RecordType::Fbi => Box::new(Fbi::from_record(stream, &mut anomalies)?),
        RecordType::BopPop => Box::new(BopPop::from_record(stream, &mut anomalies)?),
        RecordType::AxcExt => Box::new(AxcExt::from_record(stream, &mut anomalies)?),
        RecordType::Dat => Box::new(Dat::from_record(stream, &mut anomalies)?),
        RecordType::PlotGrowth => Box::new(PlotGrowth::from_record(stream, &mut anomalies)?),
        RecordType::SIIndex => Box::new(SIIndex::from_record(stream, &mut anomalies)?),
        RecordType::GelFrame => Box::new(GelFrame::from_record(stream, &mut anomalies)?),
        RecordType::BopPopCustom => Box::new(BopPopCustom::from_record(stream, &mut anomalies)?),
        RecordType::Fbi2 => Box::new(Fbi2::from_record(stream, &mut anomalies)?),
        RecordType::Unsupported => return Err("Unsupported record type".into()),
    };
    Ok(r)
}

/// Type of BIFF record
#[allow(clippy::upper_case_acronyms, missing_docs)]
#[repr(u16)]
#[derive(Debug, FromPrimitive, PartialEq, Clone, Copy)]
pub enum RecordType {
    Formula = 6,
    EOF = 10,
    CalcCount = 12,
    CalcMode = 13,
    CalcPrecision = 14,
    CalcRefMode = 15,
    CalcDelta = 16,
    CalcIter = 17,
    Protect = 18,
    Password = 19,
    Header = 20,
    Footer = 21,
    ExternSheet = 23,
    Lbl = 24,
    WinProtect = 25,
    VerticalPageBreaks = 26,
    HorizontalPageBreaks = 27,
    Note = 28,
    Selection = 29,
    Date1904 = 34,
    ExternName = 35,
    LeftMargin = 38,
    RightMargin = 39,
    TopMargin = 40,
    BottomMargin = 41,
    PrintRowCol = 42,
    PrintGrid = 43,
    FilePass = 47,
    Font = 49,
    PrintSize = 51,
    Continue = 60,
    Window1 = 61,
    Backup = 64,
    Pane = 65,
    CodePage = 66,
    Pls = 77,
    DCon = 80,
    DConRef = 81,
    DConName = 82,
    DefColWidth = 85,
    XCT = 89,
    CRN = 90,
    FileSharing = 91,
    WriteAccess = 92,
    Obj = 93,
    Uncalced = 94,
    CalcSaveRecalc = 95,
    Template = 96,
    Intl = 97,
    ObjProtect = 99,
    ColInfo = 125,
    Guts = 128,
    WsBool = 129,
    GridSet = 130,
    HCenter = 131,
    VCenter = 132,
    BoundSheet8 = 133,
    WriteProtect = 134,
    Country = 140,
    HideObj = 141,
    Sort = 144,
    Palette = 146,
    Sync = 151,
    LPr = 152,
    DxGCol = 153,
    FnGroupName = 154,
    FilterMode = 155,
    BuiltInFnGroupCount = 156,
    AutoFilterInfo = 157,
    AutoFilter = 158,
    Scl = 160,
    Setup = 161,
    ScenMan = 174,
    SCENARIO = 175,
    SxView = 176,
    Sxvd = 177,
    SXVI = 178,
    SxIvd = 180,
    SXLI = 181,
    SXPI = 182,
    DocRoute = 184,
    RecipName = 185,
    MulRk = 189,
    MulBlank = 190,
    Mms = 193,
    SXDI = 197,
    SXDB = 198,
    SXFDB = 199,
    SXDBB = 200,
    SXNum = 201,
    SxBool = 202,
    SxErr = 203,
    SXInt = 204,
    SXString = 205,
    SXDtr = 206,
    SxNil = 207,
    SXTbl = 208,
    SXTBRGIITM = 209,
    SxTbpg = 210,
    ObProj = 211,
    SXStreamID = 213,
    DBCell = 215,
    SXRng = 216,
    SxIsxoper = 217,
    BookBool = 218,
    DbOrParamQry = 220,
    ScenarioProtect = 221,
    OleObjectSize = 222,
    XF = 224,
    InterfaceHdr = 225,
    InterfaceEnd = 226,
    SXVS = 227,
    MergeCells = 229,
    BkHim = 233,
    MsoDrawingGroup = 235,
    MsoDrawing = 236,
    MsoDrawingSelection = 237,
    PhoneticInfo = 239,
    SxRule = 240,
    SXEx = 241,
    SxFilt = 242,
    SxDXF = 244,
    SxItm = 245,
    SxName = 246,
    SxSelect = 247,
    SXPair = 248,
    SxFmla = 249,
    SxFormat = 251,
    SST = 252,
    LabelSst = 253,
    ExtSST = 255,
    SXVDEx = 256,
    SXFormula = 259,
    SXDBEx = 290,
    RRDInsDel = 311,
    RRDHead = 312,
    RRDChgCell = 315,
    RRTabId = 317,
    RRDRenSheet = 318,
    RRSort = 319,
    RRDMove = 320,
    RRFormat = 330,
    RRAutoFmt = 331,
    RRInsertSh = 333,
    RRDMoveBegin = 334,
    RRDMoveEnd = 335,
    RRDInsDelBegin = 336,
    RRDInsDelEnd = 337,
    RRDConflict = 338,
    RRDDefName = 339,
    RRDRstEtxp = 340,
    LRng = 351,
    UsesELFs = 352,
    DSF = 353,
    CUsr = 401,
    CbUsr = 402,
    UsrInfo = 403,
    UsrExcl = 404,
    FileLock = 405,
    RRDInfo = 406,
    BCUsrs = 407,
    UsrChk = 408,
    UserBView = 425,
    UserSViewBegin = 426,
    //UserSViewBegin_Chart = 436
    UserSViewEnd = 427,
    RRDUserView = 428,
    Qsi = 429,
    SupBook = 430,
    Prot4Rev = 431,
    CondFmt = 432,
    CF = 433,
    DVal = 434,
    DConBin = 437,
    TxO = 438,
    RefreshAll = 439,
    HLink = 440,
    Lel = 441,
    CodeName = 442,
    SXFDBType = 443,
    Prot4RevPass = 444,
    ObNoMacros = 445,
    Dv = 446,
    Excel9File = 448,
    RecalcId = 449,
    EntExU2 = 450,
    Dimensions = 512,
    Blank = 513,
    Number = 515,
    Label = 516,
    BoolErr = 517,
    String = 519,
    Row = 520,
    Index = 523,
    Array = 545,
    DefaultRowHeight = 549,
    Table = 566,
    Window2 = 574,
    RK = 638,
    Style = 659,
    BigName = 1048,
    Format = 1054,
    ContinueBigName = 1084,
    ShrFmla = 1212,
    HLinkTooltip = 2048,
    WebPub = 2049,
    QsiSXTag = 2050,
    DBQueryExt = 2051,
    ExtString = 2052,
    TxtQry = 2053,
    Qsir = 2054,
    Qsif = 2055,
    RRDTQSIF = 2056,
    BOF = 2057,
    OleDbConn = 2058,
    WOpt = 2059,
    SXViewEx = 2060,
    SXTH = 2061,
    SXPIEx = 2062,
    SXVDTEx = 2063,
    SXViewEx9 = 2064,
    ContinueFrt = 2066,
    RealTimeData = 2067,
    ChartFrtInfo = 2128,
    FrtWrapper = 2129,
    StartBlock = 2130,
    EndBlock = 2131,
    StartObject = 2132,
    EndObject = 2133,
    CatLab = 2134,
    YMult = 2135,
    SXViewLink = 2136,
    PivotChartBits = 2137,
    FrtFontList = 2138,
    SheetExt = 2146,
    BookExt = 2147,
    SXAddl = 2148,
    CrErr = 2149,
    HFPicture = 2150,
    FeatHdr = 2151,
    Feat = 2152,
    DataLabExt = 2154,
    DataLabExtContents = 2155,
    CellWatch = 2156,
    FeatHdr11 = 2161,
    Feature11 = 2162,
    DropDownObjIds = 2164,
    ContinueFrt11 = 2165,
    DConn = 2166,
    List12 = 2167,
    Feature12 = 2168,
    CondFmt12 = 2169,
    CF12 = 2170,
    CFEx = 2171,
    XFCRC = 2172,
    XFExt = 2173,
    AutoFilter12 = 2174,
    ContinueFrt12 = 2175,
    MDTInfo = 2180,
    MDXStr = 2181,
    MDXTuple = 2182,
    MDXSet = 2183,
    MDXProp = 2184,
    MDXKPI = 2185,
    MDB = 2186,
    PLV = 2187,
    Compat12 = 2188,
    DXF = 2189,
    TableStyles = 2190,
    TableStyle = 2191,
    TableStyleElement = 2192,
    StyleExt = 2194,
    NamePublish = 2195,
    NameCmt = 2196,
    SortData = 2197,
    Theme = 2198,
    GUIDTypeLib = 2199,
    FnGrp12 = 2200,
    NameFnGrp12 = 2201,
    MTRSettings = 2202,
    CompressPictures = 2203,
    HeaderFooter = 2204,
    CrtLayout12 = 2205,
    CrtMlFrt = 2206,
    CrtMlFrtContinue = 2207,
    ForceFullCalculation = 2211,
    ShapePropsStream = 2212,
    TextPropsStream = 2213,
    RichTextStream = 2214,
    CrtLayout12A = 2215,
    Units = 4097,
    Chart = 4098,
    Series = 4099,
    DataFormat = 4102,
    LineFormat = 4103,
    MarkerFormat = 4105,
    AreaFormat = 4106,
    PieFormat = 4107,
    AttachedLabel = 4108,
    SeriesText = 4109,
    ChartFormat = 4116,
    Legend = 4117,
    SeriesList = 4118,
    Bar = 4119,
    Line = 4120,
    Pie = 4121,
    Area = 4122,
    Scatter = 4123,
    CrtLine = 4124,
    Axis = 4125,
    Tick = 4126,
    ValueRange = 4127,
    CatSerRange = 4128,
    AxisLine = 4129,
    CrtLink = 4130,
    DefaultText = 4132,
    Text = 4133,
    FontX = 4134,
    ObjectLink = 4135,
    Frame = 4146,
    Begin = 4147,
    End = 4148,
    PlotArea = 4149,
    Chart3d = 4154,
    PicF = 4156,
    DropBar = 4157,
    Radar = 4158,
    Surf = 4159,
    RadarArea = 4160,
    AxisParent = 4161,
    LegendException = 4163,
    ShtProps = 4164,
    SerToCrt = 4165,
    AxesUsed = 4166,
    SBaseRef = 4168,
    SerParent = 4170,
    SerAuxTrend = 4171,
    IFmtRecord = 4174,
    Pos = 4175,
    AlRuns = 4176,
    BRAI = 4177,
    SerAuxErrBar = 4187,
    ClrtClient = 4188,
    SerFmt = 4189,
    Chart3DBarShape = 4191,
    Fbi = 4192,
    BopPop = 4193,
    AxcExt = 4194,
    Dat = 4195,
    PlotGrowth = 4196,
    SIIndex = 4197,
    GelFrame = 4198,
    BopPopCustom = 4199,
    Fbi2 = 4200,
    #[num_enum(default)]
    Unsupported,
}

impl RecordType {
    /// Converts u16 to RecordType and warns about invalid values
    pub fn new(value: u16) -> Self {
        let result = RecordType::from_primitive(value);
        if result == RecordType::Unsupported {
            warn!("Unsupported record type: 0x{value:02X}");
        }
        result
    }
}

/// The AlRuns record specifies Rich Text Formatting within chart titles, trendline, and data labels.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::AlRuns)]
pub struct AlRuns {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Area record specifies that the chart group (section 2.2.3.7) is an area chart group (section 2.2.3.7) and specifies the chart group (section 2.2.3.7) attributes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Area)]
pub struct Area {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The AreaFormat record specifies the patterns and colors used in a filled region of a chart (section 2.2.3.3).
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::AreaFormat)]
pub struct AreaFormat {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Array record specifies an array formula (section 2.2.2) for a range of cells that performs calculations on one or more sets of values, and then returns either a single result or multiple results across a continuous range of cells.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Array)]
pub struct Array {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The AttachedLabel record specifies properties of a data label (section 2.2.3.11) on a chart group (section 2.2.3.7), series (section 2.2.3.9), or data point (section 2.2.3.10).
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::AttachedLabel)]
pub struct AttachedLabel {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The AutoFilter record specifies a mechanism that can be used to filter tabular data based on user-defined criteria such as values, strings, and formatting.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::AutoFilter)]
pub struct AutoFilter {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The AutoFilter12 record specifies AutoFilter properties.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::AutoFilter12)]
pub struct AutoFilter12 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The AutoFilterInfo record specifies the number of columns that have AutoFilter enabled and specifies the beginning of a collection of records as defined by the Macro Sheet Substream ABNF and Worksheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::AutoFilterInfo)]
pub struct AutoFilterInfo {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The AxcExt record specifies additional extension properties of a date axis (section 2.2.3.6), along with a CatSerRange record (section 2.4.39).
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::AxcExt)]
pub struct AxcExt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The AxesUsed record specifies the number of axis groups (section 2.2.3.5) on the chart (section 2.2.3.3).
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::AxesUsed)]
pub struct AxesUsed {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Axis record specifies properties of an axis (section 2.2.3.6) and specifies the beginning of a collection of records as defined by the Chart Sheet Substream ABNF that specifies an axis (section 2.2.3.6).
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Axis)]
pub struct Axis {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The AxisLine record specifies which part of the axis (section 2.2.3.6) is specified by the LineFormat record (section 2.4.156) that follows.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::AxisLine)]
pub struct AxisLine {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The AxisParent record specifies properties of an axis group (section 2.2.3.5) and specifies the beginning of a collection of records as defined by the Chart Sheet Substream ABNF that specifies an axis group (section 2.2.3.5).
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::AxisParent)]
pub struct AxisParent {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Backup record specifies whether to save a backup copy of the workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Backup)]
pub struct Backup {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Bar record specifies that the chart group (section 2.2.3.7) is a bar chart group (section 2.2.3.7) or a column chart group (section 2.2.3.7), and specifies the chart group (section 2.2.3.7) attributes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Bar)]
pub struct Bar {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The BCUsrs record specifies the beginning of a collection of UsrInfo records (section 2.4.340) as defined the user names stream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::BCUsrs)]
pub struct BCUsrs {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Begin record specifies the beginning of a collection of records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Begin)]
pub struct Begin {
    //empty record
}

/// The BigName record specifies a name/value pair of arbitrary user-defined data that is associated with the current sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::BigName)]
pub struct BigName {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The BkHim record specifies image data for a sheet (1) background.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::BkHim)]
pub struct BkHim {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Blank record specifies an empty cell with no formula (section 2.2.2) or value.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Blank)]
pub struct Blank {
    /// A Cell structure that specifies the cell.
    pub cell: Cell,
}

/// The BOF record specifies the beginning of the individual substreams as specified by the workbook section. It also specifies history information for the substreams.
#[derive(Debug)]
pub struct Bof {
    /// specifies the BIFF version of the file.
    pub vers: u16,
    /// specifies the document type of the substream of records following this record.
    pub dt: BofType,
    /// specifies the build identifier.
    pub rup_build: u16,
    /// specifies the year when this BIFF version was first created.
    pub rup_year: u16,
    /// specifies whether this file was last edited on a Windows platform.
    pub f_win: bool,
    /// specifies whether the file was last edited on a Reduced Instruction Set Computing (RISC) platform.
    pub f_risc: bool,
    /// specifies whether this file was last edited by a beta version of the application.
    pub f_beta: bool,
    /// specifies whether this file has ever been edited on a Windows platform.
    pub f_win_any: bool,
    /// specifies whether this file has ever been edited on a Macintosh platform.
    pub f_mac_any: bool,
    /// specifies whether this file has ever been edited by a beta version of the application.
    pub f_beta_any: bool,
    /// specifies whether this file has ever been edited on a RISC platform.
    pub f_risc_any: bool,
    /// specifies whether this file had an out-of-memory failure.
    pub f_oom: bool,
    /// specifies whether this file had an out-of-memory failure during rendering.
    pub f_gl_jmp: bool,
    /// specified that whether this file hit the 255 font limit
    pub f_font_limit: bool,
    /// specifies the highest version of the application that once saved this file.
    pub ver_xlhigh: ExcelVersion,
    /// specifies the BIFF version saved.
    pub ver_lowest_biff: u8,
    /// specifies the application that saved this file most recently.
    pub ver_last_xlsaved: ExcelVersion,
}

impl FromRecordStream for Bof {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: std::marker::Sized,
    {
        debug!("Bof::from_record({stream:?})");
        check_record_type!(stream, RecordType::BOF);
        // Note: BOF is not encrypted
        let mut non_decrypting_stream = stream.get_ndr();
        let mut anomalies: Vec<String> = Vec::new();
        let vers = rdu16le(&mut non_decrypting_stream)?;
        if vers != 0x600 {
            anomalies.push(format!("Invalid BOF::vers {:x}", vers));
        }
        let dt = match rdu16le(&mut non_decrypting_stream)? {
            0x05 => BofType::Workbook,
            0x10 => BofType::DialogOrWorksheet,
            0x20 => BofType::ChartSheet,
            0x40 => BofType::MacroSheet,
            x => {
                anomalies.push(format!("Invalid BOF::dt {x:x}"));
                BofType::Invalid(x)
            }
        };
        let rup_build = rdu16le(&mut non_decrypting_stream)?;
        let rup_year = rdu16le(&mut non_decrypting_stream)?;
        if rup_year != 0x7cc && rup_year != 0x7cd {
            anomalies.push(format!("Invalid BOF::rupYear {:x}", rup_year));
        }
        let misc1 = rdu32le(&mut non_decrypting_stream)?;
        let misc2 = rdu32le(&mut non_decrypting_stream)?;

        Ok(Self {
            vers,
            dt,
            rup_build,
            rup_year,
            f_win: (misc1 & (1 << 0)) != 0,
            f_risc: (misc1 & (1 << 1)) != 0,
            f_beta: (misc1 & (1 << 2)) != 0,
            f_win_any: (misc1 & (1 << 3)) != 0,
            f_mac_any: (misc1 & (1 << 4)) != 0,
            f_beta_any: (misc1 & (1 << 5)) != 0,
            f_risc_any: (misc1 & (1 << 8)) != 0,
            f_oom: (misc1 & (1 << 9)) != 0,
            f_gl_jmp: (misc1 & (1 << 10)) != 0,
            f_font_limit: (misc1 & (1 << 13)) != 0,
            ver_xlhigh: ExcelVersion::new(((misc1 >> 14) & 0x0f) as u8),
            ver_lowest_biff: (misc2 & 0xff) as u8,
            ver_last_xlsaved: ExcelVersion::new(((misc2 >> 8) & 0x0f) as u8),
        })
    }
}

impl Record for Bof {
    fn record_type() -> RecordType {
        RecordType::BOF
    }
}

/// The BookBool record specifies some of the properties associated with a workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::BookBool)]
pub struct BookBool {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The BookExt record specifies properties of a workbook file.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::BookExt)]
pub struct BookExt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The BoolErr record specifies a cell that contains either a Boolean value or an error value.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::BoolErr)]
pub struct BoolErr {
    /// A Cell structure that specifies the cell.
    pub cell: Cell,
    /// A Bes structure that specifies a Boolean or an error value.
    pub bes: Bes,
}

/// The BopPop record specifies that the chart group is a bar of pie chart group or a pie of pie chart group and specifies the chart group attributes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::BopPop)]
pub struct BopPop {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The BopPopCustom record specifies which data points in the series are contained in the secondary bar/pie instead of the primary pie.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::BopPopCustom)]
pub struct BopPopCustom {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The BottomMargin record specifies the bottom margin of the current sheet (1).
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::BottomMargin)]
pub struct BottomMargin {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The BoundSheet8 record specifies basic information about a sheet.
#[derive(Debug)]
pub struct BoundSheet8 {
    /// A FilePointer as specified in [MS-OSHARED] section 2.2.1.5 that specifies the stream position of the start of the BOF record for the sheet.
    pub lb_ply_pos: u32,
    /// Hidden state of the sheet.
    pub hs_state: HSState,
    /// Sheet type
    pub dt: SheetType,
    /// specifies the unique case-insensitive name of the sheet
    pub st_name: ShortXLUnicodeString,
}

impl FromRecordStream for BoundSheet8 {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        check_record_type!(stream, RecordType::BoundSheet8);
        Ok(Self {
            lb_ply_pos: rdu32le(&mut stream.get_ndr())?, // NOT encrypted
            hs_state: HSState::from_record(stream, anomalies)?,
            dt: SheetType::from_record(stream, anomalies)?,
            st_name: ShortXLUnicodeString::from_record(stream, anomalies)?,
        })
    }
}

impl Record for BoundSheet8 {
    fn record_type() -> RecordType {
        RecordType::BoundSheet8
    }
}

/// The BRAI record specifies a reference to data in a sheet that is used by a part of a series, legend entry, trendline or error bars.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::BRAI)]
#[allow(clippy::upper_case_acronyms)]
pub struct BRAI {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The BuiltInFnGroupCount record specifies the beginning of a collection of records as defined by the Globals Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::BuiltInFnGroupCount)]
pub struct BuiltInFnGroupCount {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CalcCount record specifies the iteration count for a calculation in iterative calculation mode.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CalcCount)]
pub struct CalcCount {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CalcDelta record specifies the minimum value change required for iterative calculation to continue.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CalcDelta)]
pub struct CalcDelta {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CalcIter record specifies the state of iterative calculation.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CalcIter)]
pub struct CalcIter {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CalcMode record specifies the calculation mode for the workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CalcMode)]
pub struct CalcMode {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CalcPrecision record specifies the calculation precision mode for the workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CalcPrecision)]
pub struct CalcPrecision {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CalcRefMode record specifies the reference style for the workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CalcRefMode)]
pub struct CalcRefMode {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CalcSaveRecalc record specifies the recalculation behavior.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CalcSaveRecalc)]
pub struct CalcSaveRecalc {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CatLab record specifies the attributes of the axis label.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CatLab)]
pub struct CatLab {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CatSerRange record specifies the properties of a category axis, a date axis, or a series axis.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CatSerRange)]
pub struct CatSerRange {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CbUsr record specifies the size of each UsrInfo record stored as part of a shared workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CbUsr)]
pub struct CbUsr {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CellWatch record specifies a reference to a watched cell.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CellWatch)]
pub struct CellWatch {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CF record specifies a conditional formatting rule.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CF)]
pub struct CF {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CF12 record specifies a conditional formatting rule. All CF12 records MUST follow a CondFmt12 record, another CF12 record, or a CFEx record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CF12)]
pub struct CF12 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CFEx record extends a CondFmt.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CFEx)]
pub struct CFEx {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Chart record specifies the position and size of the chart area (section 2.2.3.17) and specifies the beginning of a collection of records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Chart)]
pub struct Chart {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Chart3d record specifies that the plot area of the chart group is rendered in a 3-D scene and also specifies the attributes of the 3-D plot area.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Chart3d)]
pub struct Chart3d {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Chart3DBarShape record specifies the shape of the data points in a bar or column chart group.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Chart3DBarShape)]
pub struct Chart3DBarShape {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ChartFormat record specifies properties of a chart group and specifies the beginning of a collection of records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ChartFormat)]
pub struct ChartFormat {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ChartFrtInfo record specifies the versions of the application that originally created and last saved the file, and the Future Record identifiers that are used in the file.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ChartFrtInfo)]
pub struct ChartFrtInfo {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ClrtClient record specifies a custom color palette for a chart.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ClrtClient)]
pub struct ClrtClient {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CodeName record specifies the name of a workbook object, a sheet object in the VBA project located in this file.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CodeName)]
pub struct CodeName {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CodePage record specifies code page information for the workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CodePage)]
pub struct CodePage {
    /// workbook’s code pag
    pub cv: u16,
}

/// The ColInfo record specifies the column formatting for a range of columns.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ColInfo)]
pub struct ColInfo {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Compat12 record specifies whether to check for compatibility with earlier application versions when saving the workbook from a version of the application to the binary formats of other versions of the application.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Compat12)]
pub struct Compat12 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CompressPictures record specifies a recommendation for picture compression when saving.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CompressPictures)]
pub struct CompressPictures {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CondFmt record specifies conditional formatting rules that are associated with a set of cells.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CondFmt)]
pub struct CondFmt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CondFmt12 record specifies conditional formatting rules that are associated with a set of cells, when all the rules are specified using CF12 records.  
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CondFmt12)]
pub struct CondFmt12 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Continue record specifies a continuation of the data in a preceding record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Continue)]
pub struct Continue {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ContinueBigName record specifies a continuation of the data in a preceding BigName record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ContinueBigName)]
pub struct ContinueBigName {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ContinueFrt record specifies a continuation of the data in a preceding Future Record Type record that has data longer than 8,224 bytes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ContinueFrt)]
pub struct ContinueFrt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ContinueFrt11 record specifies a continuation of the data in a preceding Future Record Type record that has data longer than 8,224 bytes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ContinueFrt11)]
pub struct ContinueFrt11 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ContinueFrt12 record specifies a continuation of the data in a preceding Future Record Type record that has data longer than 8,224 bytes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ContinueFrt12)]
pub struct ContinueFrt12 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// Provides mapping from county/region code to its name
#[derive(FromRecordStream)]
#[from_record(Struct)]
pub struct CountryOrRegion {
    code: u16,
}
fn country_or_region_to_string(country: &CountryOrRegion) -> String {
    match country.code {
        1 => "United States",
        2 => "Canada",
        3 => "Latin America, except Brazil",
        7 => "Russia",
        20 => "Egypt",
        30 => "Greece",
        31 => "Netherlands",
        32 => "Belgium",
        33 => "France",
        34 => "Spain",
        36 => "Hungary",
        39 => "Italy",
        41 => "Switzerland",
        43 => "Austria",
        44 => "United Kingdom",
        45 => "Denmark",
        46 => "Sweden",
        47 => "Norway",
        48 => "Poland",
        49 => "Germany",
        52 => "Mexico",
        55 => "Brazil",
        61 => "Australia",
        64 => "New Zealand",
        66 => "Thailand",
        81 => "Japan",
        82 => "Korea",
        84 => "Viet Nam",
        86 => "People’s Republic of China",
        90 => "Türkiye",
        213 => "Algeria",
        216 => "Morocco",
        218 => "Libya",
        351 => "Portugal",
        354 => "Iceland",
        358 => "Finland",
        420 => "Czech Republic",
        886 => "Taiwan",
        961 => "Lebanon",
        962 => "Jordan",
        963 => "Syria",
        964 => "Iraq",
        965 => "Kuwait",
        966 => "Saudi Arabia",
        971 => "United Arab Emirates",
        972 => "Israel",
        974 => "Qatar",
        981 => "Iran",
        other => return format!("Invalid ({other})"),
    }
    .to_string()
}

impl fmt::Debug for CountryOrRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'{}'", country_or_region_to_string(self))
    }
}

/// The Country record specifies locale information for a workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Country)]
pub struct Country {
    /// specifies the country/region code determined by the locale in effect when the workbook was saved.
    pub country_def: CountryOrRegion,
    /// specifies the system regional settings country/region code in effect when the workbook was saved.
    pub country_win_ini: CountryOrRegion,
}

/// The CrErr record specifies the errors detected during crash recovery of a workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CrErr)]
pub struct CrErr {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CRN record specifies the values of cells in a sheet in an external cell cache.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CRN)]
#[allow(clippy::upper_case_acronyms)]
pub struct CRN {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CrtLayout12 record specifies the layout information for attached label, when contained in the sequence of records that conforms to the ATTACHEDLABEL rule, or legend, when contained in the sequence of records that conforms to the LD rule.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CrtLayout12)]
pub struct CrtLayout12 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CrtLayout12A record specifies layout information for a plot area.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CrtLayout12A)]
pub struct CrtLayout12A {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CrtLine record specifies the presence of drop lines, high-low lines, series lines or leader lines on the chart group.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CrtLine)]
pub struct CrtLine {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CrtLink record is written but unused.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CrtLink)]
pub struct CrtLink {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CrtMlFrt record specifies additional properties for chart elements, as specified by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CrtMlFrt)]
pub struct CrtMlFrt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CrtMlFrtContinue record specifies additional data for a CrtMlFrt record, as specified in the CrtMlFrt record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CrtMlFrtContinue)]
pub struct CrtMlFrtContinue {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The CUsr record specifies the number of unique users that have this shared workbook open.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::CUsr)]
pub struct CUsr {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Dat record specifies the beginning of a collection of records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Dat)]
pub struct Dat {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DataFormat record specifies the data point or series that the formatting information that follows applies to and specifies the beginning of a collection of records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DataFormat)]
pub struct DataFormat {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DataLabExt record specifies the beginning of a collection of records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DataLabExt)]
pub struct DataLabExt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DataLabExtContents record specifies the contents of an extended data label.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DataLabExtContents)]
pub struct DataLabExtContents {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Date1904 record specifies the date system that the workbook uses.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Date1904)]
pub struct Date1904 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DBCell record specifies a row block, which is a series of up to 32 consecutive rows.  
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DBCell)]
pub struct DBCell {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DbOrParamQry record specifies a DbQuery or ParamQry record depending on the record that precedes this record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DbOrParamQry)]
pub struct DbOrParamQry {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DbQuery record specifies information about an external connection.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DBQueryExt)]
pub struct DBQueryExt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DBQueryExt record specifies information about an external connection.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DCon)]
pub struct DCon {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DConBin record specifies a built-in named range that is a data source for a PivotTable or a data source for the data consolidation settings of the associated sheet.  
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DConBin)]
pub struct DConBin {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DCon record specifies the data consolidation settings of the associated sheet and specifies the beginning of a collection of records as defined by the Macro Sheet Substream ABNF and Worksheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DConn)]
pub struct DConn {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DConName record specifies a named range that is a data source for a PivotTable or a data source for the data consolidation settings of the associated sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DConName)]
pub struct DConName {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DConRef record specifies a range in this workbook or in an external workbook that is a data source for a PivotTable or a data source for the data consolidation settings of the associated sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DConRef)]
pub struct DConRef {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DefaultRowHeight record specifies the height of all empty rows in the current sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DefaultRowHeight)]
pub struct DefaultRowHeight {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DefaultText record specifies the text elements that are formatted using the information specified by the Text record immediately following this record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DefaultText)]
pub struct DefaultText {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DefColWidth record specifies the default column width of a sheet and specifies the beginning of a collection of ColInfo records as defined by the Macro Sheet Substream ABNF and Worksheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DefColWidth)]
pub struct DefColWidth {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Dimensions record specifies the used range of the sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Dimensions)]
pub struct Dimensions {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DocRoute record specifies the document routing information for a routing slip that is used to send a document in an e-mail message and specifies the beginning of a collection of RecipName records as defined by the Globals Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DocRoute)]
pub struct DocRoute {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DropBar record specifies the attributes of the up bars or the down bars between multiple series of a line chart group and specifies the beginning of a collection of records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DropBar)]
pub struct DropBar {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DropDownObjIds record specifies the object identifiers that can be reused by the application when creating the dropdown objects for the AutoFilter at runtime in a sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DropDownObjIds)]
pub struct DropDownObjIds {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DSF record is reserved and MUST be ignored.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DSF)]
#[allow(clippy::upper_case_acronyms)]
pub struct DSF {
    /// MUST be zero, and MUST be ignored.
    pub reserved: u16,
}

/// The Dv record specifies a single set of data validation criteria defined for a range on this sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Dv)]
pub struct Dv {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DVal record specifies data validation information that is common to all cells in a sheet that have data validation applied and specifies the beginning of a collection of Dv records as defined by the Worksheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DVal)]
pub struct DVal {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DXF record specifies a differential format.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DXF)]
#[allow(clippy::upper_case_acronyms)]
pub struct DXF {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The DxGCol record specifies the default column width for all sheet columns that do not have a column width explicitly specified.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::DxGCol)]
pub struct DxGCol {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The End record specifies the end of a collection of records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::End)]
pub struct End {
    //empty record
}

/// The EndBlock record specifies the end of a collection of records.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::EndBlock)]
pub struct EndBlock {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The EndObject record specifies properties of an Future Record Type (FRT) as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::EndObject)]
pub struct EndObject {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The EntExU2 record specifies an application-specific cache of information.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::EntExU2)]
pub struct EntExU2 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The EOF record specifies the end of a collection of records as defined by Globals Substream ABNF, Worksheet Substream ABNF, Dialog Sheet Substream ABNF, Chart Sheet Substream ABNF, macro sheet substream ABNF, revision stream ABNF, and pivot cache storage ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::EOF)]
#[allow(clippy::upper_case_acronyms)]
pub struct EOF {
    //empty record
}

/// The Excel9File record is optional and is unused.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Excel9File)]
pub struct Excel9File {
    //empty record
}

/// The ExternName record specifies an external defined name, a User Defined Function (UDF) reference on a XLL or COM add-in, a DDE data item or an OLE data item, depending on the value of the virtPath field in the preceding SupBook record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ExternName)]
pub struct ExternName {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ExternSheet record specifies a collection of XTI structures.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ExternSheet)]
pub struct ExternSheet {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ExtSST record specifies the location of sets of strings within the shared string table, specified in the SST record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ExtSST)]
pub struct ExtSST {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ExtString record specifies the connection string for a query that retrieves external data.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ExtString)]
pub struct ExtString {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Fbi record specifies the font information at the time the scalable font is added to the chart.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Fbi)]
pub struct Fbi {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Fbi2 record specifies the font information at the time the scalable font is added to the chart.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Fbi2)]
pub struct Fbi2 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Feat record specifies Shared Feature data.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Feat)]
pub struct Feat {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The FeatHdr record specifies common information for Shared Features and specifies the beginning of a collection of records as defined by the Globals Substream ABNF, macro sheet substream  ABNF and worksheet substream  ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::FeatHdr)]
pub struct FeatHdr {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The FeatHdr11 record specifies common information for all tables on a sheet and specifies the beginning of a collection as specified by the Worksheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::FeatHdr11)]
pub struct FeatHdr11 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Feature11 record specifies specific shared feature data
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Feature11)]
pub struct Feature11 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Feature12 record specifies shared feature data that is used to describe a table in a worksheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Feature12)]
pub struct Feature12 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The FileLock record specifies that the shared workbook was locked by a particular user.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::FileLock)]
pub struct FileLock {
    /// Placeholder for record data
    pub data: UnparsedUnencryptedData,
}

/// The FilePass record specifies the encryption algorithm used to encrypt the workbook and the structure that is used to verify the password provided when attempting to open the workbook. If this record exists, the workbook MUST be encrypted.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::FilePass)]
pub struct FilePass {
    /// Placeholder for record data
    pub data: UnparsedUnencryptedData,
}

/// The FileSharing record specifies file sharing options.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::FileSharing)]
pub struct FileSharing {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The FilterMode record specifies that the containing sheet data was filtered.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::FilterMode)]
pub struct FilterMode {
    //empty record
}

/// The FnGroupName record specifies a user-defined function category in the current workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::FnGroupName)]
pub struct FnGroupName {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The FnGrp12 record specifies the name of a user-defined function category in the current workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::FnGrp12)]
pub struct FnGrp12 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Font record specifies a font and font formatting information.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Font)]
pub struct Font {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The FontX record specifies the font for a given text element.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::FontX)]
pub struct FontX {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Footer record specifies the footer text of the current sheet when printed.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Footer)]
pub struct Footer {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ForceFullCalculation record specifies the value of the forced calculation mode for this workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ForceFullCalculation)]
pub struct ForceFullCalculation {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Format record specifies a number format.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Format)]
pub struct Format {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Formula record specifies a formula (section 2.2.2) for a cell.
#[derive(Debug)]
pub struct Formula {
    /// A Cell structure that specifies the cell.
    pub cell: Cell,
    /// A FormulaValue structure that specifies the value of the formula.
    pub val: FormulaValue,
    // Ignore other Formula fields
}

impl Record for Formula {
    fn record_type() -> RecordType
    where
        Self: Sized,
    {
        RecordType::Formula
    }
}

impl FromRecordStream for Formula {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        check_record_type!(stream, RecordType::Formula);
        let cell = Cell::from_record(stream, anomalies)?;
        let val = FormulaValue::from_record(stream, anomalies)?;

        if !matches!(val, FormulaValue::String(_)) {
            return Ok(Formula { cell, val });
        }

        loop {
            if !stream.next()? {
                return Err("Unable to find Formula String value".into());
            }
            match stream.ty {
                RecordType::String => {
                    // TODO: Verify stream.rd_xlu_string
                    let string = StringR::from_record(stream, anomalies)?;
                    let val = FormulaValue::String(string.string.to_string());
                    return Ok(Formula { cell, val });
                }
                RecordType::Table
                | RecordType::Array
                | RecordType::ShrFmla
                | RecordType::Uncalced => {}
                _ => return Err("Unable to find Formula String value".into()),
            }
        }
    }
}

/// The Frame record specifies the type, size and position of the frame around a chart element as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Frame)]
pub struct Frame {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The FrtFontList record specifies font information used on the chart and specifies the beginning of a collection of Font records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::FrtFontList)]
pub struct FrtFontList {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The FrtWrapper record wraps around a non-Future Record Type (FRT) record and converts it into an FRT record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::FrtWrapper)]
pub struct FrtWrapper {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The GelFrame record specifies the properties of a fill pattern for parts of a chart.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::GelFrame)]
pub struct GelFrame {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The GridSet record specifies a reserved value.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::GridSet)]
pub struct GridSet {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The GUIDTypeLib record specifies the GUID as specified by [MS-DTYP] that uniquely identifies the type library of the application that wrote the Visual Basic for Applications (VBA) project in the file.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::GUIDTypeLib)]
pub struct GUIDTypeLib {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Guts record specifies the maximum outline levels for row and column gutters.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Guts)]
pub struct Guts {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The HCenter record specifies whether the sheet is to be centered horizontally when printed.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::HCenter)]
pub struct HCenter {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Header record specifies the header text of the current sheet when printed.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Header)]
pub struct Header {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The HeaderFooter record specifies the even page header and footer text, and the first page header and footer text of the current sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::HeaderFooter)]
pub struct HeaderFooter {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The HFPicture record specifies a picture used by a sheet header or footer.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::HFPicture)]
pub struct HFPicture {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The HideObj record specifies how ActiveX objects, OLE objects, and drawing objects appear in a window that contains the workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::HideObj)]
pub struct HideObj {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The HLink record specifies a hyperlink associated with a range of cells.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::HLink)]
pub struct HLink {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The HLinkTooltip record specifies the hyperlink ToolTip associated with a range of cells.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::HLinkTooltip)]
pub struct HLinkTooltip {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The HorizontalPageBreaks record specifies a list of explicit row page breaks.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::HorizontalPageBreaks)]
pub struct HorizontalPageBreaks {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The IFmtRecord record specifies the number format to use for the text on an axis.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::IFmtRecord)]
pub struct IFmtRecord {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Index record specifies row information and the file locations for all DBCell records corresponding to each row block in the sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Index)]
pub struct Index {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The InterfaceEnd record specifies the end of a collection of records as defined by the Globals Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::InterfaceEnd)]
pub struct InterfaceEnd {
    //empty record
}

/// The InterfaceHdr record specifies the code page of the user interface and specifies the beginning of a collection of records as defined by the Globals Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::InterfaceHdr)]
pub struct InterfaceHdr {
    /// Placeholder for record data
    pub data: UnparsedUnencryptedData,
}

/// The Intl record specifies that the macro sheet is an international macro sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Intl)]
pub struct Intl {
    /// MUST be zero, and MUST be ignored.
    pub reserved: u16,
}

/// The Label record specifies a label on the category (2) axis for each series.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Label)]
pub struct Label {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The LabelSst record specifies a cell that contains a string.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::LabelSst)]
pub struct LabelSst {
    /// A Cell structure that specifies the cell.
    pub cell: Cell,
    ///  An unsigned integer that specifies the zero-based index of an element in the array of XLUnicodeRichExtendedString structure in the rgb field of the SST record in this Workbook Stream ABNF that specifies the string contained in the cell.
    pub isst: u32,
}

/// The Lbl record specifies a defined name.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Lbl)]
pub struct Lbl {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The LeftMargin record specifies the left margin of the current sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::LeftMargin)]
pub struct LeftMargin {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Legend record specifies properties of a legend, and specifies the beginning of a collection of records defined by Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Legend)]
pub struct Legend {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The LegendException record specifies information about a legend entry which was changed from the default legend entry settings, and specifies the beginning of a collection of records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::LegendException)]
pub struct LegendException {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Lel record specifies that a natural language formula was lost because of the deletion of a supporting label.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Lel)]
pub struct Lel {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Line record specifies that the chart group is a line chart group, and specifies the chart group attributes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Line)]
pub struct Line {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The LineFormat record specifies the appearance of a line.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::LineFormat)]
pub struct LineFormat {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The List12 record specifies the additional formatting information for a table.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::List12)]
pub struct List12 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The LPr record specifies a record that is unused.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::LPr)]
pub struct LPr {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The LRng record specifies a label range for natural language formulas.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::LRng)]
pub struct LRng {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MarkerFormat record specifies the color, size, and shape of the associated data markers that appear on line, radar, and scatter chart groups.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MarkerFormat)]
pub struct MarkerFormat {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MDB record specifies a unique set of MDX metadata type/value pairs that are shared among all cells in the workbook that reference MDX metadata.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MDB)]
#[allow(clippy::upper_case_acronyms)]
pub struct MDB {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MDTInfo record specifies the information about a single type of metadata.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MDTInfo)]
pub struct MDTInfo {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MDXKPI  record specifies MDX KPI metadata.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MDXKPI)]
#[allow(clippy::upper_case_acronyms)]
pub struct MDXKPI {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MDXProp record specifies member property MDX metadata.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MDXProp)]
pub struct MDXProp {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MDXSet record specifies MDX set metadata.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MDXSet)]
pub struct MDXSet {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MDXStr record specifies a shared text string used by records specifying MDX metadata.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MDXStr)]
pub struct MDXStr {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MDXTuple record specifies MDX tuple metadata.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MDXTuple)]
pub struct MDXTuple {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MergeCells record specifies merged cells in the document.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MergeCells)]
pub struct MergeCells {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Mms record is reserved and MUST be ignored.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Mms)]
pub struct Mms {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MsoDrawing record specifies a drawing.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MsoDrawing)]
pub struct MsoDrawing {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MsoDrawingGroup record specifies a group of drawing objects.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MsoDrawingGroup)]
pub struct MsoDrawingGroup {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MsoDrawingSelection record specifies selected drawing objects and the drawing objects in focus on the sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MsoDrawingSelection)]
pub struct MsoDrawingSelection {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MTRSettings record specifies multithreaded calculation settings.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MTRSettings)]
pub struct MTRSettings {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MulBlank record specifies a series of blank cells in a sheet row.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::MulBlank)]
pub struct MulBlank {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The MulRk record specifies a series of cells with numeric data in a sheet row.
#[derive(Debug)]
pub struct MulRk {
    /// specifies the row containing the cells with numeric data.
    pub rw: u16,
    /// specifies the first column in the series of numeric cells within the sheet.
    pub col_first: u16,
    /// An array of RkRec structures.
    pub rgrkrec: Vec<RkRec>,
    /// specifies the last column in the set of numeric cells within the sheet.
    pub col_last: u16,
}

impl FromRecordStream for MulRk {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        check_record_type!(stream, RecordType::MulRk);
        let rw = stream.rdu16()?;
        let col_first = stream.rdu16()?;
        let mut rgrkrec = Vec::<RkRec>::new();
        while stream.sz - stream.pos > 2 {
            let rkrec = RkRec::from_record(stream, anomalies)?;
            rgrkrec.push(rkrec);
        }
        let col_last = stream.rdu16()?;
        Ok(Self {
            rw,
            col_first,
            rgrkrec,
            col_last,
        })
    }
}

impl Record for MulRk {
    fn record_type() -> RecordType {
        RecordType::MulRk
    }
}

/// The NameCmt record specifies a comment associated with a defined name.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::NameCmt)]
pub struct NameCmt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The NameFnGrp12 record specifies the name of a function in a function category that is specified in an FnGrp12 record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::NameFnGrp12)]
pub struct NameFnGrp12 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The NamePublish record specifies information about a defined name.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::NamePublish)]
pub struct NamePublish {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Note record specifies a comment associated with a cell or revision information about a comment associated with a cell.  
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Note)]
pub struct Note {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Number record specifies a cell that contains a floating-point number.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Number)]
pub struct Number {
    /// A Cell structure that specifies the cell.
    pub cell: Cell,
    /// An Xnum (section 2.5.342) value that specifies the cell value.
    pub num: f64,
}

/// The Obj record specifies the properties of an object in a sheet.
#[derive(Debug)]
pub struct Obj {
    /// An FtCmo structure that specifies the common properties of this object.
    pub cmo: FtCmo,
    /// An optional FtGmo structure that specifies the properties of this group object.
    pub gmo: Option<FtGmo>,
    /// An optional FtCf structure that specifies the format of this picture object.
    pub pict_format: Option<FtCf>,
    /// An optional FtPioGrbit structure that specifies additional properties of this picture object.
    pub pict_flags: Option<FtPioGrbit>,
    /// An optional FtCbls structure that represents a check box or radio button.
    pub cbls: Option<FtCbls>,
    /// An optional FtRbo structure that represents a radio button.
    pub rbo: Option<FtRbo>,
    /// An optional FtSbs structure that specifies the properties of this spin control, scrollbar, list, or drop-down list object.
    pub sbs: Option<FtSbs>,
    /// An optional FtNts structure that specifies the properties of this comment object.
    pub nts: Option<FtNts>,
    /// An optional FtMacro structure that specifies the action associated with this object.
    pub macro_: Option<FtMacro>,
    /// An optional FtPictFmla structure that specifies the location of the data associated with this picture object.
    pub pict_fmla: Option<FtPictFmla>,
    /// An optional ObjLinkFmla structure that specifies the formula (section 2.2.2) that specifies a range that has a value linked to this object.
    pub link_fmla: Option<ObjLinkFmla>,
    /// An optional FtCblsData structure that specifies the properties of this check box or radio button object.
    pub check_box: Option<FtCblsData>,
    /// An optional FtRboData structure that specifies additional properties of this radio button object.
    pub radio_button: Option<FtRboData>,
    /// An optional FtEdoData structure that specifies the properties of this edit box object.
    pub edit: Option<FtEdoData>,
    /// An optional FtLbsData structure that specifies the properties of this list box or drop-down object.
    pub list: Option<FtLbsData>,
    /// An optional FtGboData structure that specifies the properties of this group box object.
    pub gbo: Option<FtGboData>,
}

impl FromRecordStream for Obj {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        let cmo = FtCmo::from_record(stream, anomalies)?;
        let mut cbls = None;
        let mut check_box = None;
        let mut edit = None;
        let mut gbo = None;
        let mut gmo = None;
        let mut link_fmla = None;
        let mut list = None;
        let mut macro_ = None;
        let mut nts = None;
        let mut pict_flags = None;
        let mut pict_fmla = None;
        let mut pict_format = None;
        let mut radio_button = None;
        let mut rbo = None;
        let mut sbs = None;

        //debug!("{:#?}", cmo);

        while let Ok(ft) = stream.rdu16() {
            match ft {
                0x0000 => {
                    stream.sink(2)?;
                    break;
                }
                0x0004 => macro_ = Some(FtMacro::from_record(stream, ft, anomalies)?),
                0x0006 => gmo = Some(FtGmo::from_record(stream, ft, anomalies)?),
                0x0007 => pict_format = Some(FtCf::from_record(stream, ft, anomalies)?),
                0x0008 => pict_flags = Some(FtPioGrbit::from_record(stream, ft, anomalies)?),
                0x0009 => {
                    pict_fmla = Some(FtPictFmla::from_record(
                        stream,
                        ft,
                        pict_flags.as_ref().ok_or("Unable to unwrap pict_flags")?,
                        anomalies,
                    )?)
                }
                0x000A => cbls = Some(FtCbls::from_record(stream, ft, anomalies)?),
                0x000B => rbo = Some(FtRbo::from_record(stream, ft, anomalies)?),
                0x000C => sbs = Some(FtSbs::from_record(stream, ft, anomalies)?),
                0x000D => nts = Some(FtNts::from_record(stream, ft, anomalies)?),
                0x000E => link_fmla = Some(ObjLinkFmla::from_record(stream, ft, &cmo, anomalies)?),
                0x000F => gbo = Some(FtGboData::from_record(stream, ft, anomalies)?),
                0x0010 => edit = Some(FtEdoData::from_record(stream, ft, anomalies)?),
                0x0011 => radio_button = Some(FtRboData::from_record(stream, ft, anomalies)?),
                0x0012 => check_box = Some(FtCblsData::from_record(stream, ft, anomalies)?),
                0x0013 => list = Some(FtLbsData::from_record(stream, ft, &cmo, anomalies)?),
                0x0014 => link_fmla = Some(ObjLinkFmla::from_record(stream, ft, &cmo, anomalies)?),

                _ => return Err(format!("Obj: Unexpected ft value 0x{ft:04x}").into()),
            }
        }

        Ok(Self {
            cbls,
            check_box,
            cmo,
            edit,
            gbo,
            gmo,
            link_fmla,
            list,
            macro_,
            nts,
            pict_flags,
            pict_fmla,
            pict_format,
            radio_button,
            rbo,
            sbs,
        })
    }
}

impl Record for Obj {
    fn record_type() -> RecordType {
        RecordType::Obj
    }
}

impl Obj {
    /// Returns embedded data location (if any)
    pub fn data_location(&self) -> Option<DataLocation> {
        let f_prsm = self.pict_flags.as_ref().map(|v| v.f_prstm)?;
        let pict_fmla = self.pict_fmla.as_ref()?;
        let l_pos_in_ctl_stm = pict_fmla.l_pos_in_ctl_stm?;
        if f_prsm {
            let cb_buf_in_ctl_stm = pict_fmla.cb_buf_in_ctl_stm?;
            Some(DataLocation::ControlStream {
                offset: l_pos_in_ctl_stm,
                size: cb_buf_in_ctl_stm,
            })
        } else {
            let storage = format!("MBD{l_pos_in_ctl_stm:08X}");
            Some(DataLocation::EmbeddingStorage { storage })
        }
    }
}

/// The ObjectLink record specifies an object on a chart, or the entire chart, to which the Text record is linked.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ObjectLink)]
pub struct ObjectLink {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ObjProtect record specifies the protection state of the objects on the sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ObjProtect)]
pub struct ObjProtect {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The existence of the ObNoMacros record specifies that an ObProj record exists in the file, and that there are no forms, modules, or class modules in the VBA project located in the VBA storage stream.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ObNoMacros)]
pub struct ObNoMacros {
    //empty record
}

/// The existence of the ObProj record specifies that there is a VBA project in the file. This project is located in the VBA storage stream.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ObProj)]
pub struct ObProj {
    //empty record
}

/// The OleDbConn record specifies the connection information for an OLE DB connection string, and specifies the beginning of a collection of ExtString records as defined by the Worksheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::OleDbConn)]
pub struct OleDbConn {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The OleObjectSize record specifies the visible range of cells when this workbook is displayed as an embedded object in another document.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::OleObjectSize)]
pub struct OleObjectSize {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Palette record specifies a custom color palette.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Palette)]
pub struct Palette {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Pane record specifies the position of frozen panes or unfrozen panes in the window used to display the sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Pane)]
pub struct Pane {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Password record specifies the password verifier for the sheet or workbook
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Password)]
pub struct Password {
    /// specifies the password verifier
    pub w_password: u16,
}

/// The PhoneticInfo record specifies the default format for phonetic strings and the ranges of cells on the sheet that have phonetic strings that are visible.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::PhoneticInfo)]
pub struct PhoneticInfo {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The PicF record specifies the layout of a picture that is attached to a picture-filled chart element.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::PicF)]
pub struct PicF {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Pie record specifies that the chart group is a pie chart group or a doughnut chart group, and specifies the chart group attributes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Pie)]
pub struct Pie {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The PieFormat record specifies the distance of a data point or data points in a series.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::PieFormat)]
pub struct PieFormat {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The PivotChartBits record specifies the flags applicable to a Pivot Chart.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::PivotChartBits)]
pub struct PivotChartBits {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The PlotArea record is empty, specifying that the Frame record that immediately follows this record specifies properties of the plot area.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::PlotArea)]
pub struct PlotArea {
    //empty record
}

/// The PlotGrowth record specifies the scale factors to use when calculating the font scaling information for a font in the plot area.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::PlotGrowth)]
pub struct PlotGrowth {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Pls record specifies printer settings and the printer driver information.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Pls)]
pub struct Pls {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The PLV record specifies the settings of a Page Layout view for a sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::PLV)]
#[allow(clippy::upper_case_acronyms)]
pub struct PLV {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Pos record specifies the size and position for a legend, an attached label, or the plot area, as specified by the primary axis group.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Pos)]
pub struct Pos {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The PrintGrid record specifies whether the gridlines are printed.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::PrintGrid)]
pub struct PrintGrid {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The PrintRowCol record specifies whether the row and column headers are printed.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::PrintRowCol)]
pub struct PrintRowCol {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The PrintSize record specifies the printed size of the chart.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::PrintSize)]
pub struct PrintSize {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Prot4Rev record specifies whether removal of the shared workbook's revision logs is disallowed.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Prot4Rev)]
pub struct Prot4Rev {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Prot4RevPass record specifies the password verifier that is required to change the value of the fRevLock field of the Prot4Rev record that immediately precedes this record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Prot4RevPass)]
pub struct Prot4RevPass {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Protect record specifies the protection state for the sheet or workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Protect)]
pub struct Protect {
    /// Specifies whether the sheet or workbook is protected.
    pub f_lock: u16,
}

/// The Qsi record specifies properties for a query table, and specifies the beginning of a collection of records as defined by the Worksheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Qsi)]
pub struct Qsi {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Qsif record specifies the properties for a query table field.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Qsif)]
pub struct Qsif {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Qsir record specifies the properties related to the formatting of a query table, and specifies the beginning of a collection of Qsif records as defined by the Worksheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Qsir)]
pub struct Qsir {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The QsiSXTag record specifies the name and refresh information for a query table or a PivotTable view, and specifies the beginning of a collection of records as defined by the Worksheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::QsiSXTag)]
pub struct QsiSXTag {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Radar record specifies that the chart group is a radar chart group and specifies the chart group attributes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Radar)]
pub struct Radar {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RadarArea record specifies that the chart group is a filled radar chart group and specifies the chart group attributes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RadarArea)]
pub struct RadarArea {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RealTimeData record specifies the real-time data (RTD) information for a workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RealTimeData)]
pub struct RealTimeData {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RecalcId record specifies the identifier of the recalculation engine that performed the last recalculation.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RecalcId)]
pub struct RecalcId {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RecipName record specifies information about a recipient of a routing slip.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RecipName)]
pub struct RecipName {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RefreshAll record specifies whether external data ranges, PivotTables  and XML maps will be refreshed on workbook load.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RefreshAll)]
pub struct RefreshAll {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RichTextStream record specifies additional text properties for the text in the entire chart, text in the current legend, text in the current legend entry, or text in the attached label.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RichTextStream)]
pub struct RichTextStream {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RightMargin record specifies the right margin of the current sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RightMargin)]
pub struct RightMargin {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RK record specifies the numeric data contained in a single cell.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RK)]
pub struct RK {
    /// Row index.
    pub rw: u16,
    /// Column index.
    pub col: u16,
    /// Numeric data for a single cell.
    pub rkrec: RkRec,
}

/// The Row record specifies a single row on a sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Row)]
pub struct Row {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRAutoFmt record specifies the changes caused by AutoFormat actions in a shared workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRAutoFmt)]
pub struct RRAutoFmt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRDChgCell record specifies a change cells revision.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDChgCell)]
pub struct RRDChgCell {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRDConflict record specifies the resolution of a conflict between the revisions of two uses.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDConflict)]
pub struct RRDConflict {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRDDefName record specifies a defined name revision.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDDefName)]
pub struct RRDDefName {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRDHead record specifies metadata about a set of revisions that a user has made in a shared workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDHead)]
pub struct RRDHead {
    /// Placeholder for record data
    pub data: UnparsedUnencryptedData,
}

/// The RRDInfo record specifies information about a shared workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDInfo)]
pub struct RRDInfo {
    /// Placeholder for record data
    pub data: UnparsedUnencryptedData,
}

/// The RRDInsDel record specifies the insertion / deletion of rows / columns revision changes, and specifies the beginning of a collection of records as defined by the Revision Stream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDInsDel)]
pub struct RRDInsDel {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRDInsDelBegin record specifies the beginning of a collection of records as defined by the Revision Stream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDInsDelBegin)]
pub struct RRDInsDelBegin {
    //empty record
}

/// The RRDInsDelEnd record specifies the end of a collection of records as defined by the Revision Stream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDInsDelEnd)]
pub struct RRDInsDelEnd {
    //empty record
}

/// The RRDMove record represents revision record information about the range of cells that have moved.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDMove)]
pub struct RRDMove {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRDMoveBegin record specifies the beginning of a collection of records as defined by the Revision Stream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDMoveBegin)]
pub struct RRDMoveBegin {
    //empty record
}

/// The RRDMoveEnd record specifies the end of a collection of records as defined by the Revision Stream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDMoveEnd)]
pub struct RRDMoveEnd {
    //empty record
}

/// The RRDRenSheet record specifies the old and new name of a sheet after renaming the sheet in a shared workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDRenSheet)]
pub struct RRDRenSheet {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRDRstEtxp record specifies font information for a formatting run. Instances of this record MUST be preceded by an RRDChgCell record that specifies the cell containing the formatting run.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDRstEtxp)]
pub struct RRDRstEtxp {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRDTQSIF record specifies the query table field that has been removed in a shared workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDTQSIF)]
#[allow(clippy::upper_case_acronyms)]
pub struct RRDTQSIF {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRDUserView record specifies the changes caused by a custom view revision in a shared workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRDUserView)]
pub struct RRDUserView {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRFormat record specifies a formatting change that was applied to a shared workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRFormat)]
pub struct RRFormat {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRInsertSh record specifies the changes caused by inserting a sheet in a shared workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRInsertSh)]
pub struct RRInsertSh {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRSort record specifies the changes caused by sort actions in a shared workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRSort)]
pub struct RRSort {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The RRTabId record specifies an array of unique sheet identifiers, each of which is associated with a sheet in the workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::RRTabId)]
pub struct RRTabId {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SBaseRef record specifies the location of a PivotTable view referenced by a chart.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SBaseRef)]
pub struct SBaseRef {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Scatter record specifies that the chart group is a scatter chart group or a bubble chart group, and specifies the chart group attributes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Scatter)]
pub struct Scatter {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SCENARIO record specifies a scenario.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SCENARIO)]
#[allow(clippy::upper_case_acronyms)]
pub struct SCENARIO {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ScenarioProtect record specifies the protection state for scenarios in a sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ScenarioProtect)]
pub struct ScenarioProtect {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ScenMan record specifies the state of the Scenario Manager for the sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ScenMan)]
pub struct ScenMan {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Scl record specifies the zoom level of the current view in the window used to display the sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Scl)]
pub struct Scl {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Selection record specifies selected cells within a sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Selection)]
pub struct Selection {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SerAuxErrBar record specifies properties of an error bar.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SerAuxErrBar)]
pub struct SerAuxErrBar {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SerAuxTrend record specifies a trendline.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SerAuxTrend)]
pub struct SerAuxTrend {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SerFmt record specifies properties of the associated data points, data markers, or lines of the series.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SerFmt)]
pub struct SerFmt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Series record specifies properties of the data for a series, a trendline, or error bars, and specifies the beginning of a collection of records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Series)]
pub struct Series {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SeriesList record specifies the series for the chart.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SeriesList)]
pub struct SeriesList {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SeriesText record specifies the text for a series, trendline name, trendline label, axis title or chart title.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SeriesText)]
pub struct SeriesText {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SerParent record specifies the series to which the current trendline or error bar corresponds.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SerParent)]
pub struct SerParent {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SerToCrt record specifies the chart group for the current series.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SerToCrt)]
pub struct SerToCrt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Setup record specifies the page format settings used to print the current sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Setup)]
pub struct Setup {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ShapePropsStream record specifies the shape formatting properties for chart elements.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ShapePropsStream)]
pub struct ShapePropsStream {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SheetExt record specifies sheet properties, including sheet tab color and additional optional information specified by using the SheetExtOptional structure.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SheetExt)]
pub struct SheetExt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ShrFmla record specifies a formula (section 2.2.2) that is shared across multiple cells.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ShrFmla)]
pub struct ShrFmla {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ShtProps record specifies properties of a chart as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ShtProps)]
pub struct ShtProps {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SIIndex record is part of a group of records which specify the data of a chart.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SIIndex)]
pub struct SIIndex {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Sort record specifies the information used to sort values contained in a range of cells.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Sort)]
pub struct Sort {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SortData record specifies data used for sorting a range.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SortData)]
pub struct SortData {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SST record specifies string constants.
#[derive(Debug)]
#[allow(clippy::upper_case_acronyms)]
pub struct SST {
    /// Specifies the total number of references in the workbook to the strings in the shared string table.
    pub cst_total: u32,
    /// Specifies the number of unique strings in the shared string table.
    pub cst_unique: u32,
    /// An array of XLUnicodeRichExtendedString structures.
    pub rgb: Vec<XLUnicodeRichExtendedString>,
}

impl FromRecordStream for SST {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("SST::from_record({stream:?})");
        let cst_total =
            u32::try_from(stream.rdi32()?).map_err(|_| "cstTotal MUST be zero or greater")?;
        let cst_unique =
            u32::try_from(stream.rdi32()?).map_err(|_| "cstUnique MUST be zero or greater")?;
        debug!("total: {cst_total}, unique: {cst_unique}");
        let mut rgb = Vec::<XLUnicodeRichExtendedString>::new();
        while rgb.len() < cst_unique as usize {
            //debug!("INDEX: {data: UnparsedData,}", rgb.len());
            let pos = stream.pos;
            let str = if let Ok(str) = XLUnicodeRichExtendedString::from_record(stream, anomalies) {
                str
            } else {
                let message = format!(
                    "SST: Unable to load string at index={} (Record pos={}, size={}).",
                    rgb.len(),
                    pos,
                    stream.sz
                );
                warn!("{}", message);
                anomalies.push(message);
                break;
            };
            rgb.push(str);
        }
        Ok(Self {
            cst_total,
            cst_unique,
            rgb,
        })
    }
}
impl Record for SST {
    fn record_type() -> RecordType {
        RecordType::SST
    }
}

/// The StartBlock record specifies the beginning of a collection of records.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::StartBlock)]
pub struct StartBlock {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The StartObject record specifies the beginning of a collection of Future Record Type records as defined by the Chart Sheet Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::StartObject)]
pub struct StartObject {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The String record specifies the string value of a formula
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::String)]
pub struct StringR {
    /// specifies the string value of a formula
    pub string: XLUnicodeString,
}

/// The Style record specifies a cell style.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Style)]
pub struct Style {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The StyleExt record specifies additional information for a cell style.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::StyleExt)]
pub struct StyleExt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SupBook record specifies a supporting link and specifies the beginning of a collection of records as defined by the Globals Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SupBook)]
pub struct SupBook {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Surf record specifies that the chart group is a surface chart group and specifies the chart group attributes.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Surf)]
pub struct Surf {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXAddl record specifies additional information for a PivotTable view, PivotCache, or query table.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXAddl)]
pub struct SXAddl {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxBool record specifies a Boolean cache item or value.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxBool)]
pub struct SxBool {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXDB record specifies PivotCache properties.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXDB)]
#[allow(clippy::upper_case_acronyms)]
pub struct SXDB {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXDBB record specifies the values of all the cache fields that have a fAllAtoms field of the SXFDB record equal to 1 and that correspond to source data entities, as specified by cache fields, for a single cache record.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXDBB)]
#[allow(clippy::upper_case_acronyms)]
pub struct SXDBB {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXDBEx record specifies additional PivotCache properties.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXDBEx)]
pub struct SXDBEx {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXDI record specifies a data item for a PivotTable view.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXDI)]
#[allow(clippy::upper_case_acronyms)]
pub struct SXDI {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXDtr record specifies a cache item or a value in the PivotCache that is an instance in time, expressed as a date and time of day.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXDtr)]
pub struct SXDtr {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxDXF record specifies differential formatting applied to a PivotTable area.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxDXF)]
pub struct SxDXF {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxErr record specifies an error cache item or value.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxErr)]
pub struct SxErr {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXEx record specifies additional properties of a PivotTable view and specifies the beginning of a collection of records as defined by the Worksheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXEx)]
pub struct SXEx {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXFDB record specifies properties for a cache field within a PivotCache.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXFDB)]
#[allow(clippy::upper_case_acronyms)]
pub struct SXFDB {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXFDBType record specifies the type of data contained in this cache field.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXFDBType)]
pub struct SXFDBType {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxFilt record specifies information for a PivotTable rule filter.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxFilt)]
pub struct SxFilt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxFmla record specifies a PivotParsedFormula and specifies the beginning of a collection of records as defined by the pivot cache storage ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxFmla)]
pub struct SxFmla {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxFormat record specifies the beginning of a collection of records as defined by the Worksheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxFormat)]
pub struct SxFormat {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXFormula record specifies the cache field that a calculated item formula (section 2.2.2) applies to.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXFormula)]
pub struct SXFormula {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXInt record specifies a number in the PivotCache.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXInt)]
pub struct SXInt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxIsxoper record specifies the mapping between cache items in a cache field and cache items in a grouping cache field for discrete grouping, as specified by Grouping.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxIsxoper)]
pub struct SxIsxoper {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxItm record specifies references to pivot items, data items, or cache items as part of a PivotTable rule filter.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxItm)]
pub struct SxItm {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxIvd record specifies an array of SxIvdRw or SxIvdCol.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxIvd)]
pub struct SxIvd {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXLI record specifies pivot lines for the row area or column area of a PivotTable view.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXLI)]
#[allow(clippy::upper_case_acronyms)]
pub struct SXLI {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxName record specifies information used for a calculated field or calculated item and that specifies the beginning of a collection of records as specified by the pivot cache storage ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxName)]
pub struct SxName {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxNil record specifies an empty cache item or value.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxNil)]
pub struct SxNil {
    //empty record
}

/// The SXNum record specifies a numeric cache item or value.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXNum)]
pub struct SXNum {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXPair record specifies a reference to a pivot item used to compute the value of a calculated item in a PivotTable.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXPair)]
pub struct SXPair {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXPI record specifies the pivot fields and information about filtering on the page axis of a PivotTable view.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXPI)]
#[allow(clippy::upper_case_acronyms)]
pub struct SXPI {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXPIEx record specifies OLAP extensions to the page axis of a PivotTable view.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXPIEx)]
pub struct SXPIEx {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXRng record specifies properties for numeric grouping or date grouping of cache items in a grouping cache field, as specified by Grouping.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXRng)]
pub struct SXRng {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxRule record specifies areas or parts of a one or more PivotTable views, as specified in PivotTable rules, and that specifies the beginning of a collection of SxFilt records as specified by the Common Productions ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxRule)]
pub struct SxRule {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxSelect record specifies information about selected cells in the PivotTable report for a PivotTable view.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxSelect)]
pub struct SxSelect {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXStreamID record specifies a stream in the PivotCache storage.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXStreamID)]
pub struct SXStreamID {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXString record specifies a segment of a string that contains information about a PivotCache or an external connection.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXString)]
pub struct SXString {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXTbl record stores information about multiple consolidation ranges.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXTbl)]
pub struct SXTbl {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxTbpg record specifies properties of source data ranges for a multiple consolidation ranges PivotCache.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxTbpg)]
pub struct SxTbpg {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXTBRGIITM record specifies the beginning of a collection of SXString records as specified by the Globals Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXTBRGIITM)]
#[allow(clippy::upper_case_acronyms)]
pub struct SXTBRGIITM {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXTH record specifies properties of a pivot hierarchy.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXTH)]
#[allow(clippy::upper_case_acronyms)]
pub struct SXTH {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Sxvd record specifies pivot field properties and that specifies the beginning of a collection of records as defined in the Worksheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Sxvd)]
pub struct Sxvd {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXVDEx record specifies extended pivot field properties.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXVDEx)]
pub struct SXVDEx {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXVDTEx record specifies OLAP extensions to a pivot field.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXVDTEx)]
pub struct SXVDTEx {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXVI record specifies information about a pivot item.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXVI)]
#[allow(clippy::upper_case_acronyms)]
pub struct SXVI {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SxView record specifies PivotTable view information and that specifies the beginning of a collection of records as defined by the Worksheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SxView)]
pub struct SxView {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXViewEx record specifies the beginning of a collection of records as specified in the Worksheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXViewEx)]
pub struct SXViewEx {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXViewEx9 record specifies extensions to the PivotTable view.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXViewEx9)]
pub struct SXViewEx9 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXViewLink record specifies the name of the source PivotTable view associated with a Pivot Chart.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXViewLink)]
pub struct SXViewLink {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The SXVS record specifies the type of source data used for a PivotCache.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::SXVS)]
#[allow(clippy::upper_case_acronyms)]
pub struct SXVS {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// When multiple windows are used to view a sheet with synchronous scrolling enabled, the Sync record specifies the coordinates of the top-left visible cell of all windows.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Sync)]
pub struct Sync {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Table record specifies a data table.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Table)]
pub struct Table {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The TableStyle record specifies a user-defined table style and the beginning of a collection of TableStyleElement records as specified by the Globals Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::TableStyle)]
pub struct TableStyle {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The TableStyleElement record specifies formatting for one element of a table style.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::TableStyleElement)]
pub struct TableStyleElement {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The TableStyles record specifies the default table and PivotTable table styles and specifies the beginning of a collection of TableStyle records as defined by the Globals Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::TableStyles)]
pub struct TableStyles {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Template record is an empty record that specifies whether the workbook is a template.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Template)]
pub struct Template {
    //empty record
}

/// The Text record specifies the properties of an attached label and specifies the beginning of a collection of records as defined by the chart sheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Text)]
pub struct Text {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The TextPropsStream record specifies additional text properties for the text in the entire chart, text in the current legend, text in the current legend entry, text in the attached label, or the axis labels of the current axis.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::TextPropsStream)]
pub struct TextPropsStream {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Theme record specifies the theme in use in the document.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Theme)]
pub struct Theme {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Tick record specifies the attributes of the axis labels, major tick marks, and minor tick marks associated with an axis.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Tick)]
pub struct Tick {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The TopMargin record specifies the top margin of the current sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::TopMargin)]
pub struct TopMargin {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The TxO record specifies the text in a text box or a form control.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::TxO)]
pub struct TxO {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The TxtQry record specifies information for a text query and that specifies the beginning of a collection of ExtString records, as defined by the Worksheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::TxtQry)]
pub struct TxtQry {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Uncalced record specifies that formulas (section 2.2.2) were pending recalculation when the file was saved.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Uncalced)]
pub struct Uncalced {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Units record MUST be zero, and MUST be ignored.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Units)]
pub struct Units {
    /// MUST be zero, and MUST be ignored.
    pub reserved: u16,
}

/// The UserBView record specifies the general custom view settings that apply to a whole workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::UserBView)]
pub struct UserBView {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The UserSViewBegin record specifies custom view settings for the current sheet and specifies the beginning of a collection of records as defined by the Chart Sheet substream ABNF, Dialog Sheet substream ABNF, Macro Sheet substream ABNF, and Worksheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::UserSViewBegin)]
pub struct UserSViewBegin {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The UserSViewBegin_Chart record specifies custom view settings for the current chart sheet and that specifies the beginning of a collection of records as defined by the Chart Sheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::UserSViewBegin)]
pub struct UserSViewBeginChart {
    /// MUST be zero, and MUST be ignored.
    pub reserved: u16,
}

/// The UserSViewEnd record specifies the end of a collection of records, as defined by the common productions substream ABNF and the Dialog Sheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::UserSViewEnd)]
pub struct UserSViewEnd {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The UsesELFs record specifies whether the file supports natural language formulas.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::UsesELFs)]
pub struct UsesELFs {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The UsrChk record specifies the version information for the last user who opened the shared workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::UsrChk)]
pub struct UsrChk {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The UsrExcl record specifies whether a user has acquired an exclusive lock on the shared workbook and that specifies the beginning of a collection of records as defined by the revision stream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::UsrExcl)]
pub struct UsrExcl {
    /// Placeholder for record data
    pub data: UnparsedUnencryptedData,
}

/// The UsrInfo record specifies information about a user who currently has the shared workbook open.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::UsrInfo)]
pub struct UsrInfo {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The ValueRange record specifies the properties of a value axis.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::ValueRange)]
pub struct ValueRange {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The VCenter record specifies whether the sheet is centered vertically when printed.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::VCenter)]
pub struct VCenter {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The VerticalPageBreaks record specifies a list of all explicit column page breaks in the sheet.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::VerticalPageBreaks)]
pub struct VerticalPageBreaks {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The WebPub record specifies the information for a single published Web page.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::WebPub)]
pub struct WebPub {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Window1 record specifies attributes of a window used to display a sheet (called "the window" within this record definition).  For each Window1 record in the Globals Substream there MUST be an associated Window2 record in each chart sheet, worksheet, macro sheet, and dialog sheet substream that exists in the workbook.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Window1)]
pub struct Window1 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The Window2 record specifies attributes of the window used to display a sheet in a workbook and that specifies the beginning of a collection of records as defined by the Chart Sheet substream ABNF, Macro Sheet substream ABNF, and Worksheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::Window2)]
pub struct Window2 {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The WinProtect record specifies whether the workbook windows can be resized or moved and whether the window state can be changed.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::WinProtect)]
pub struct WinProtect {
    /// specifies whether the windows can be resized or moved and whether the window state can be changed.
    pub f_lock_win: u16,
}

/// The WOpt record specifies options for saving as a Web page.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::WOpt)]
pub struct WOpt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The WriteAccess record specifies the name of the user who last created, opened, or modified the file.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::WriteAccess)]
pub struct WriteAccess {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The existence of the WriteProtect record specifies that the file is write-protected.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::WriteProtect)]
pub struct WriteProtect {
    //empty record
}

/// The WsBool record specifies information about a sheet.
#[derive(Debug)]
pub struct WsBool {
    /// specifies whether page breaks inserted automatically are visible on the sheet.
    pub f_show_auto_breaks: bool,
    /// specifies whether the sheet is a dialog sheet.
    pub f_dialog: bool,
    /// specifies whether to apply styles in an outline when an outline is applied.
    pub f_apply_styles: bool,
    /// specifies whether summary rows appear below an outline's detail rows.
    pub f_row_sums_below: bool,
    /// specifies whether summary columns appear to the right or left of an outline's detail columns.
    pub f_col_sums_right: bool,
    /// specifies whether to fit the printable contents to a single page when printing this sheet.
    pub f_fit_to_page: bool,
    /// specifies whether horizontal scrolling is synchronized across multiple windows displaying this sheet.
    pub f_sync_horiz: bool,
    /// specifies whether vertical scrolling is synchronized across multiple windows displaying this sheet.         
    pub f_sync_vert: bool,
    /// specifies whether the sheet uses transition formula evaluation.
    pub f_alt_expr_eval: bool,
    /// specifies whether the sheet uses transition formula entry.
    pub f_alt_formula_entry: bool,
}

impl FromRecordStream for WsBool {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("WsBool::from_record({stream:?})");
        check_record_type!(stream, RecordType::WsBool);

        let misc = stream.rdu16()?;

        let f_show_auto_breaks = (misc & 0x01) != 0;
        let reserved1 = (misc >> 1) & 0x03;
        if reserved1 != 0 {
            anomalies.push("WsBool::reserved1 is not zero".to_string());
        }
        let f_dialog = ((misc >> 4) & 0x01) != 0;
        let f_apply_styles = ((misc >> 5) & 0x01) != 0;
        let f_row_sums_below = ((misc >> 6) & 0x01) != 0;
        let f_col_sums_right = ((misc >> 7) & 0x01) != 0;
        let f_fit_to_page = ((misc >> 8) & 0x01) != 0;
        let reserved2 = (misc >> 9) & 0x01;
        if reserved2 != 0 {
            anomalies.push("WsBool::reserved2 is not zero".to_string());
        }
        let f_sync_horiz = ((misc >> 12) & 0x01) != 0;
        let f_sync_vert = ((misc >> 13) & 0x01) != 0;
        let f_alt_expr_eval = ((misc >> 14) & 0x01) != 0;
        let f_alt_formula_entry = ((misc >> 15) & 0x01) != 0;

        //stream.next()?;

        Ok(WsBool {
            f_show_auto_breaks,
            f_dialog,
            f_apply_styles,
            f_row_sums_below,
            f_col_sums_right,
            f_fit_to_page,
            f_sync_horiz,
            f_sync_vert,
            f_alt_expr_eval,
            f_alt_formula_entry,
        })
    }
}

impl Record for WsBool {
    fn record_type() -> RecordType {
        RecordType::WsBool
    }
}

/// The XCT record specifies the beginning of an external cell cache and that specifies the beginning of a collection of CRN records as defined in the Globals Substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::XCT)]
#[allow(clippy::upper_case_acronyms)]
pub struct XCT {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The XF record specifies formatting properties for a cell or a cell style.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::XF)]
pub struct XF {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The XFCRC record specifies the number of XF records contained in this file and  that contains a checksum of the data in those records.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::XFCRC)]
#[allow(clippy::upper_case_acronyms)]
pub struct XFCRC {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The XFExt record specifies a set of formatting property extensions to an XF record in this file.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::XFExt)]
pub struct XFExt {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// The YMult record specifies properties of the value multiplier for a value axis and that specifies the beginning of a collection of records as defined by the Chart Sheet substream ABNF.
#[derive(Debug, FromRecordStream)]
#[from_record(Record, RecordType::YMult)]
pub struct YMult {
    /// Placeholder for record data
    pub data: UnparsedData,
}

/// Raw record data
#[derive(Debug)]
pub struct UnparsedData(pub Vec<u8>);

impl FromRecordStream for UnparsedData {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        let mut data = Vec::<u8>::new();
        stream.read_to_end(&mut data)?;
        Ok(Self(data))
    }
}

/// Raw record data (read from not decrypted reader)
#[derive(Debug)]
pub struct UnparsedUnencryptedData(pub Vec<u8>);

impl FromRecordStream for UnparsedUnencryptedData {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        let mut data = Vec::<u8>::new();
        stream.get_ndr().read_to_end(&mut data)?;
        Ok(Self(data))
    }
}
