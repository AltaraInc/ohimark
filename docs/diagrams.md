# Diagrams

A gallery of example Mermaid diagrams, one for each diagram type. Every example is a
fenced `mermaid` block in the Markdown source, rendered to inline SVG at build time.

## Codeblock
```bash
> echo $SUPER_SECRET_KEY
> echo "Done deal!"
```

## Flowchart

A flowchart shows the steps of a process and the decisions that route between them.

```mermaid
flowchart TD
  SRC["Markdown sources"] --> PARSE["Parse front matter"]
  PARSE --> DRAFT{"Marked as draft?"}
  DRAFT -->|"yes"| SKIP["Skip the page"]
  DRAFT -->|"no"| MD
  subgraph RENDER["Render"]
    MD["Convert Markdown"] --> HL["Highlight code"]
    HL --> TPL["Apply page template"]
  end
  TPL --> LINKS{"Do all links resolve?"}
  LINKS -->|"yes"| WRITE["Write HTML"]
  LINKS -.->|"no"| WARN["Report broken links"]
  WARN --> WRITE
  WRITE ==> DONE(["Site ready"])
```

## Sequence diagram

A sequence diagram shows the messages that pass between participants, in the order they
are sent.

```mermaid
sequenceDiagram
  autonumber
  actor Reader
  participant CDN
  participant Origin as Origin server
  participant Store as Object store
  Reader->>CDN: GET /guide/install.html
  alt cached and fresh
    CDN-->>Reader: 200 OK (cache hit)
  else stale or missing
    CDN->>+Origin: GET /guide/install.html
    Origin->>+Store: Fetch object
    Store-->>-Origin: Page bytes
    Origin-->>-CDN: 200 OK with ETag
    Note over CDN,Origin: The response is cached for one hour
    CDN-->>Reader: 200 OK (cache miss)
  end
  loop Each stylesheet and script
    Reader->>CDN: GET asset
    CDN-->>Reader: 200 OK
  end
```

## State diagram

A state diagram shows the states a system can be in and the events that move it from one
to the next.

```mermaid
stateDiagram-v2
  [*] --> Queued
  Queued --> Building : worker available
  state Building {
    [*] --> Discovering
    Discovering --> Rendering : sources found
    Rendering --> Copying : pages rendered
    Copying --> [*]
  }
  state outcome <<choice>>
  Building --> outcome
  outcome --> Published : no errors
  outcome --> Failed : any error
  Failed --> Queued : retry
  Published --> [*]
  note right of Failed
    The previous site stays live
  end note
```

## Gantt diagram

A Gantt diagram lays tasks out against a calendar, showing how long each takes and what it
depends on.

```mermaid
gantt
  title Documentation site launch
  dateFormat YYYY-MM-DD
  axisFormat %d %b
  excludes weekends
  todayMarker off
  section Content
  Audit existing pages  :done, audit, 2025-03-03, 4d
  Rewrite the guides    :active, guides, after audit, 8d
  Review and copy-edit  :review, after guides, 4d
  section Tooling
  Choose a generator    :done, tool, 2025-03-03, 2d
  Build the theme       :crit, theme, after tool, 6d
  Set up CI             :ci, after theme, 3d
  section Launch
  Redirect old URLs     :crit, redirects, after review ci, 2d
  Go live               :milestone, live, after redirects, 0d
```

## Swimlanes diagram

A swimlanes diagram is a flowchart divided into lanes, showing who owns each step and
where work is handed over.

```mermaid
swimlane-beta LR
  subgraph Author
    draft[Write draft]
    revise[Address comments]
  end
  subgraph Reviewer
    review{Changes needed?}
    approve[Approve change]
  end
  subgraph CI
    build[Build site]
    check[Check links]
  end
  subgraph Hosting
    publish([Publish])
  end
  draft --> review
  review -->|Yes| revise
  review -->|No| approve
  revise --> approve
  approve --> build --> check --> publish
```

## Mindmap

A mindmap arranges ideas as a tree that branches outwards from one central topic.

```mermaid
mindmap
  root((Documentation site))
    Content
      Guides
      Reference
      Changelog
    Authoring
      Markdown
      Diagrams
      Code samples
    Build
      [Link checking]
      [Syntax highlighting]
      [Search index]
    Delivery
      (CDN)
      (Redirects)
      (Versioned URLs)
```

## Timeline diagram

A timeline diagram sets events out in chronological order, grouped into periods.

```mermaid
timeline
  title History of a documentation site
  section Foundations
    2022 : First public pages : Hand-written HTML
    2023 : Moved to Markdown : Added a sidebar
  section Growth
    2024 : Full-text search : Dark theme
    2025 : Versioned pages : Diagrams : Translations
  section Planned
    2026 : Offline reading : Interactive examples
```

## Sankey

A Sankey diagram shows how a quantity flows between stages, with the width of each link
proportional to its volume.

```mermaid
sankey
Search engines,Guides,420
Search engines,Reference,310
Direct links,Guides,150
Direct links,Changelog,60
Site search,Reference,180
Guides,Reference,200
Guides,Left the site,370
Reference,Code samples,170
Reference,Left the site,520
Changelog,Left the site,60
```

## Packet diagram

A packet diagram shows the layout of a binary structure, such as a network packet, field
by field.

```mermaid
packet
title TCP segment header
0-15: "Source port"
16-31: "Destination port"
32-63: "Sequence number"
64-95: "Acknowledgement number"
96-99: "Data offset"
100-103: "Reserved"
104: "CWR"
105: "ECE"
106: "URG"
107: "ACK"
108: "PSH"
109: "RST"
110: "SYN"
111: "FIN"
112-127: "Window"
128-143: "Checksum"
144-159: "Urgent pointer"
160-191: "Options and padding"
192-255: "Data (variable length)"
```

## Architecture diagram

An architecture diagram shows the services that make up a system, how they are grouped,
and how they connect.

```mermaid
architecture-beta
  group edge(cloud)[Edge]
  group app(server)[Application]
  group data(database)[Data]

  service readers(internet)[Readers]
  service cdn(cloud)[CDN] in edge
  service lb(server)[Load balancer] in edge
  service web(server)[Web server] in app
  service search(server)[Search API] in app
  service pages(disk)[Static pages] in data
  service index(database)[Search index] in data
  junction split in app

  readers:R --> L:cdn
  cdn:R --> L:lb
  lb:R -- L:split
  split:T --> B:web
  split:B --> T:search
  web:R --> L:pages
  search:R --> L:index
```
