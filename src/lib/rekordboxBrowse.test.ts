// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { parseRekordboxBrowse, parseRekordboxBrowseWidths } from "./rekordboxBrowse";

describe("rekordbox browse settings", () => {
  it("imports supported visible columns in source order and their widths per view", () => {
    const xml = `<PROPERTIES>
      <VALUE name="TableHeader-PlaylistTracks"><TABLELAYOUT>
        <COLUMN id="20" visible="1" width="67"/>
        <COLUMN id="68" visible="1" width="200"/>
        <COLUMN id="60" visible="0" width="80"/>
        <COLUMN id="21" visible="1" width="351"/>
        <COLUMN id="33" visible="1" width="128"/>
        <COLUMN id="29" visible="1" width="80"/>
      </TABLELAYOUT></VALUE>
      <VALUE name="TableHeader-CollectionTracks"><TABLELAYOUT>
        <COLUMN id="21" visible="1" width="527"/>
        <COLUMN id="24" visible="1" width="128"/>
      </TABLELAYOUT></VALUE>
    </PROPERTIES>`;
    const parsed = parseRekordboxBrowse(xml);
    expect(parsed.playlist?.order).toEqual(["preview", "title", "bpm"]);
    expect(parsed.playlist?.widths).toMatchObject({ trackNo: 67, preview: 200, artwork: 80, title: 351 });
    expect(parsed.collection?.order).toEqual(["title", "genre"]);
    expect(parsed.collection?.widths.title).toBe(527);
    expect(parsed.history).toBeUndefined();
  });

  it("ignores malformed XML and incomplete layouts", () => {
    expect(parseRekordboxBrowse("<PROPERTIES><VALUE>")).toEqual({});
    expect(parseRekordboxBrowse('<VALUE name="TableHeader-PlaylistTracks"><COLUMN id="68" visible="1"/></VALUE>')).toEqual({});
  });

  it("reads browser pane widths without accepting zero or implausible values", () => {
    expect(parseRekordboxBrowseWidths('<BROWSELAYOUT><BROWSE comp="TreeView" w="396"/><BROWSE comp="SubBrowse" w="882"/></BROWSELAYOUT>'))
      .toEqual({ treeWidth: 396, subWidth: 882 });
    expect(parseRekordboxBrowseWidths('<BROWSELAYOUT><BROWSE comp="TreeView" w="0"/><BROWSE comp="SubBrowse" w="99999"/></BROWSELAYOUT>'))
      .toEqual({});
  });
});
