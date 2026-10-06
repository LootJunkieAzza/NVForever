// export_inventory.pas -- FNVEdit (xEdit) script: write fnv.esm.inventory/1 JSON goldens.
//
// HOW TO USE
//   1. Copy this file into FNVEdit's "Edit Scripts" folder.
//   2. Start FNVEdit, load FalloutNV.esm and the DLC masters you want to export.
//   3. In the left tree, select one or more PLUGINS (the top-level file nodes, e.g.
//      "[00] FalloutNV.esm"), right-click -> Apply Script... -> export_inventory.
//   4. Output goes to OutputDir below (default C:\fnv-private\goldens\esm\), a folder
//      OUTSIDE the repository and outside the game folder; edit the constant to change it.
//      One file per plugin is written as <PluginWithoutExtension>.xedit.inventory.json,
//      e.g. FalloutNV.xedit.inventory.json.
//   4b. Afterwards run: python tools\capture\xedit\finish_export.py C:\fnv-private\goldens\esm
//      (converts the files to UTF-8 JSON; the lab cannot read them before that).
//   5. FalloutNV.esm has 465,016 records: expect several minutes. Progress appears in the
//      Messages tab.
//
// The output is PRIVATE ground truth derived from copyrighted data: never commit it.
//
// IMPORTANT FOR EDITORS OF THIS FILE: use only // line comments. Do not use brace
// comments: a closing brace inside comment text (for example in a JSON sample) ends the
// comment early and FNVEdit reports "'unit' expected but ',' found".
//
// KNOWN UNCERTAINTIES (the script was written without access to FNVEdit; please report):
//   * FormIDs come from FixedFormID (file-local: top byte = index in that plugin's own
//     master list). Check the "FormID check" line in the Messages tab: for a DLC, almost
//     all records must be "own" or "overrides", not "out of range". Set FormIDSource
//     below to 1 (FormID) if that is not the case.
//   * GunRunnersArsenal.esm has one REFR stored as 0x02000801 with only one master; xEdit
//     will probably show it as 0x01000801. Expect that ONE difference.
//   * Record order is xEdit's own order; the lab compares by FormID, so order is irrelevant.
//   * The "header" object is best effort; the lab compares xEdit exports on records only
//     (type, edid, full).
//   * FalloutNV.esm LAND 0x00150FC0 has a bad zlib checksum; xEdit may log an error for it.

unit userscript;

const
  // Output folder: PRIVATE, outside the repository and outside the game folder. Must end
  // with a backslash.
  OutputDir = 'C:\fnv-private\goldens\esm\';
  // 0 = FixedFormID (default, see note 1), 1 = FormID, 2 = GetLoadOrderFormID (wrong; for
  // diagnostics only).
  FormIDSource = 0;
  ExportHeader = True;

var
  gPluginNames: TStringList;
  gOutputDir: string;

// ------------------------------------------------------------------------- JSON helpers

function JsonEscape(s: string): string;
var
  r: string;
begin
  // FNVEdit's interpreter cannot take Ord() of s[i] ("Type mismatch" [observed]), so
  // escape with StringReplace only. Non-ASCII characters are written as they are, in
  // the file's ANSI encoding; tools/capture/xedit/finish_export.py turns the file into
  // UTF-8 JSON for the lab (it also accepts other control characters).
  r := StringReplace(s, '\', '\\', [rfReplaceAll]);
  r := StringReplace(r, '"', '\"', [rfReplaceAll]);
  r := StringReplace(r, #13, '\r', [rfReplaceAll]);
  r := StringReplace(r, #10, '\n', [rfReplaceAll]);
  r := StringReplace(r, #9, '\t', [rfReplaceAll]);
  Result := '"' + r + '"';
end;

function JsonStrOrNull(present: boolean; s: string): string;
begin
  if present then
    Result := JsonEscape(s)
  else
    Result := 'null';
end;

function HexId(v: Int64): string;
begin
  // "0x%08X": upper-case hex digits, lower-case 0x prefix.
  Result := '0x' + UpperCase(IntToHex(v, 8));
end;

// ------------------------------------------------------------------------- record data

function FileLocalFormID(rec: IInterface): Int64;
begin
  if FormIDSource = 1 then
    Result := FormID(rec)
  else if FormIDSource = 2 then
    Result := GetLoadOrderFormID(rec)
  else
    Result := FixedFormID(rec);
end;

function RecordLine(rec: IInterface; fid: Int64): string;
var
  edid, full, fv: string;
  flags: Int64;
begin
  edid := JsonStrOrNull(ElementExists(rec, 'EDID'), EditorID(rec));
  full := JsonStrOrNull(ElementExists(rec, 'FULL'), GetElementEditValues(rec, 'FULL'));
  flags := GetElementNativeValues(rec, 'Record Header\Record Flags');
  if ElementExists(rec, 'Record Header\Form Version') then
    fv := IntToStr(GetElementNativeValues(rec, 'Record Header\Form Version'))
  else
    fv := 'null';
  Result := ' {"formid": ' + JsonEscape(HexId(fid))
    + ', "type": ' + JsonEscape(Signature(rec))
    + ', "edid": ' + edid
    + ', "full": ' + full
    + ', "flags": ' + IntToStr(flags)
    + ', "form_version": ' + fv + '}';
end;

function HeaderJson(f: IInterface): string;
var
  hdr, onam, child: IInterface;
  i, onamCount: integer;
  masters, others, sig, version, isMaster: string;
  hflags, nrec, nextid: Int64;
begin
  hdr := ElementByIndex(f, 0);
  if not Assigned(hdr) or (Signature(hdr) <> 'TES4') then begin
    Result := 'null';
    Exit;
  end;
  hflags := GetElementNativeValues(hdr, 'Record Header\Record Flags');
  nrec := GetElementNativeValues(hdr, 'HEDR\Number of Records');
  nextid := GetElementNativeValues(hdr, 'HEDR\Next Object ID');
  version := StringReplace(FloatToStr(GetElementNativeValues(hdr, 'HEDR\Version')), ',', '.', [rfReplaceAll]);

  masters := '';
  for i := 0 to MasterCount(f) - 1 do begin
    if masters <> '' then masters := masters + ', ';
    masters := masters + JsonEscape(GetFileName(MasterByIndex(f, i)));
  end;

  others := '';
  for i := 0 to ElementCount(hdr) - 1 do begin
    child := ElementByIndex(hdr, i);
    sig := Signature(child);
    if (Length(sig) = 4) and (sig <> 'HEDR') and (sig <> 'CNAM') and (sig <> 'SNAM')
       and (sig <> 'MAST') and (sig <> 'DATA') and (sig <> 'ONAM') then begin
      if others <> '' then others := others + ', ';
      others := others + JsonEscape(sig);
    end;
  end;

  onam := ElementBySignature(hdr, 'ONAM');
  if Assigned(onam) then
    onamCount := ElementCount(onam)
  else
    onamCount := 0;
  if GetIsESM(f) then
    isMaster := 'true'
  else
    isMaster := 'false';

  Result := '{"plugin": ' + JsonEscape(GetFileName(f))
    + ', "is_master": ' + isMaster
    + ', "flags": ' + IntToStr(hflags)
    + ', "version": ' + version
    + ', "num_records": ' + IntToStr(nrec)
    + ', "next_object_id": ' + JsonEscape(HexId(nextid))
    + ', "author": ' + JsonStrOrNull(ElementExists(hdr, 'CNAM'), GetElementEditValues(hdr, 'CNAM'))
    + ', "description": ' + JsonStrOrNull(ElementExists(hdr, 'SNAM'), GetElementEditValues(hdr, 'SNAM'))
    + ', "masters": [' + masters + ']'
    + ', "overridden_forms": ' + IntToStr(onamCount)
    + ', "other_subrecords": [' + others + ']'
    + ', "file_size": null}';
end;

// ------------------------------------------------------------------------- export

procedure ExportPlugin(f: IInterface);
var
  sl: TStringList;
  i, total, own, overrides, outOfRange, mc: integer;
  rec: IInterface;
  fid, idx: Int64;
  line, fileName, outPath: string;
begin
  fileName := GetFileName(f);
  mc := MasterCount(f);
  AddMessage('Exporting ' + fileName + ' (' + IntToStr(RecordCount(f)) + ' records incl. header)...');
  sl := TStringList.Create;
  try
    sl.Add('{');
    sl.Add(' "schema": "fnv.esm.inventory/1",');
    sl.Add(' "plugin": ' + JsonEscape(fileName) + ',');
    if ExportHeader then
      sl.Add(' "header": ' + HeaderJson(f) + ',')
    else
      sl.Add(' "header": null,');
    sl.Add(' "errors": [],');
    sl.Add(' "records": [');
    total := 0; own := 0; overrides := 0; outOfRange := 0;
    line := '';
    for i := 0 to RecordCount(f) - 1 do begin
      rec := RecordByIndex(f, i);
      if Signature(rec) <> 'TES4' then begin
        fid := FileLocalFormID(rec);
        idx := fid div 16777216;   // top byte
        if idx = mc then
          Inc(own)
        else if idx < mc then
          Inc(overrides)
        else
          Inc(outOfRange);
        // Emit the previous line with a trailing comma; the last one is added after the loop.
        if line <> '' then
          sl.Add(line + ',');
        line := RecordLine(rec, fid);
        Inc(total);
        if (total mod 50000) = 0 then
          AddMessage('  ' + IntToStr(total) + ' records');
      end;
    end;
    if line <> '' then
      sl.Add(line);
    sl.Add(' ]');
    sl.Add('}');
    outPath := gOutputDir + ChangeFileExt(fileName, '') + '.xedit.inventory.json';
    sl.SaveToFile(outPath);
    AddMessage('  wrote ' + IntToStr(total) + ' records to ' + outPath);
    AddMessage('  FormID check: ' + IntToStr(own) + ' own (top byte ' + IntToStr(mc) + '), '
      + IntToStr(overrides) + ' overrides/injected (top byte < ' + IntToStr(mc) + '), '
      + IntToStr(outOfRange) + ' out of range (top byte > ' + IntToStr(mc) + ').');
    if outOfRange > 1 then
      AddMessage('  WARNING: many out-of-range FormIDs -- the FormID function probably returns '
        + 'load-order IDs; see note 1 in the script header.');
  finally
    sl.Free;
  end;
end;

function Initialize: integer;
begin
  gPluginNames := TStringList.Create;
  gPluginNames.Sorted := True;
  gPluginNames.Duplicates := dupIgnore;
  // No folder dialog: InputQuery showed its default garbled and returned an empty string
  // in FNVEdit [observed]. Edit OutputDir at the top of this file instead.
  gOutputDir := OutputDir;
  try
    ForceDirectories(gOutputDir);
  except
    AddMessage('Could not create ' + gOutputDir + ' (create it yourself if it does not exist).');
  end;
  AddMessage('Writing to ' + gOutputDir);
  Result := 0;
end;

function Process(e: IInterface): integer;
begin
  // Called for every selected element (and everything under a selected plugin). Only
  // remember which plugins were touched; the export walks each whole plugin in Finalize.
  gPluginNames.Add(GetFileName(GetFile(e)));
  Result := 0;
end;

function Finalize: integer;
var
  i: integer;
  f: IInterface;
begin
  for i := 0 to FileCount - 1 do begin
    f := FileByIndex(i);
    if gPluginNames.IndexOf(GetFileName(f)) >= 0 then
      ExportPlugin(f);
  end;
  if gPluginNames.Count = 0 then
    AddMessage('Nothing selected: select one or more plugins in the left pane, then apply the script.')
  else
    // No trailing backslash: in "C:\dir\" the \" would reach Python as a literal quote.
    AddMessage('Done. Now run: python tools\capture\xedit\finish_export.py "'
      + Copy(gOutputDir, 1, Length(gOutputDir) - 1) + '"');
  gPluginNames.Free;
  Result := 0;
end;

end.
