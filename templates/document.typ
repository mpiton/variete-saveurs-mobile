#let data = json("document.json")

#let red = rgb("#C0182B")
#let brown = rgb("#4A2C1A")
#let cream = rgb("#F6F0E2")
#let pink = rgb("#EBA4AE")
#let gold = rgb("#C49A45")
#let gray = rgb("#6B5A4E")
#let rule = rgb("#ECE3CF")
#let serif = "Liberation Serif"
#let sans = "Liberation Sans"

#set document(title: data.title + " n° " + str(data.number) + " - Variété de Saveurs")
#set page(
  paper: "a4",
  margin: (top: 10mm, right: 10mm, bottom: 14mm, left: 10mm),
  footer: context align(center)[Page #counter(page).display("1 / 1", both: true)],
)
#set text(font: sans, size: 8.25pt, fill: brown, lang: "fr")
// document.css says `line-height: 1.32`, i.e. baseline to baseline. Typst's
// `leading` is the gap left of that after the cap height (`top-edge` defaults to
// "cap-height", `bottom-edge` to "baseline"), so the CSS number has to lose the
// cap height before it lands here: 1.32em - 0.688em (Liberation Sans cap height)
// = 0.632em. Writing 0.32em, the CSS leftover, made every block 31 % too tight.
#set par(leading: 0.632em)
// Block spacing adds to the explicit `#v()` gaps ported from the CSS margins, so
// the 1.2em default put an extra 9.9pt — 30.6pt after the 25.5pt title — between
// every pair of blocks. It cannot be zero either: a block ends at its cap height
// here and at its full line box in CSS, and the leading is exactly that
// difference, so the same 0.632em completes the box at each boundary.
#set par(spacing: 0.632em)
#set block(spacing: 0.632em)
// Rows the CSS separates with a per-line margin (`.party div`, `.meta div`,
// `.payment-block div`, `.conditions li` — 1 to 2px depending on the block) are
// joined by `\` here, which only leaves the leading. The margin is folded into
// it instead: 0.632em + 1.5px = 0.766em.
#let row-leading = 0.766em
// A table cell is only as tall as its cap height in Typst, where CSS gives it a
// full line box plus the row border. The inset carries the difference, so it is
// not the raw CSS padding: (2 × 3.5px + 14.52px + 1px - 0.688em) / 2 = 5.6pt.
#let cell-inset-y = 5.6pt

#let card(body) = block(
  width: 100%,
  fill: cream,
  stroke: 0.75pt + gold,
  radius: 3pt,
  inset: (x: 8.25pt, y: 5.25pt),
  body,
)

#grid(
  columns: (1fr, 1fr),
  column-gutter: 12pt,
  [
    #image("logo.png", height: 55.5pt)
    #v(2pt)
    #text(font: serif, size: 11.25pt)[Variété de Saveurs]
  ],
  align(right)[
    #text(font: serif, size: 25.5pt, weight: "bold", fill: red, tracking: 2.25pt)[#data.title]
    #v(2pt)
    #text(font: serif, size: 9pt, style: "italic", fill: gray)[#data.nature]
    #v(4.5pt)
    #text(size: 8.25pt)[
      #set par(leading: row-leading)
      *#data.number-label :* #data.number \
      *Date d'émission :* #data.issue-date \
      *Date de l'événement :* #data.event-date \
      #if data.quote [
        *Validité de l'offre :* jusqu'au #data.validity-end
      ] else [
        *Conditions de paiement :* #data.payment-terms
      ]
    ]
  ],
)
#v(5pt)
#line(length: 100%, stroke: 0.75pt + gold)
#v(6.75pt)

#grid(
  columns: (1fr, 1fr),
  column-gutter: 10.5pt,
  card([
    #text(font: serif, size: 9.4pt, weight: "bold", fill: red, tracking: 0.45pt)[ÉMETTEUR]
    #v(3.75pt)
    #set par(leading: row-leading)
    #text(size: 9.4pt, weight: "bold")[Variété de Saveurs] \
    2 impasse du printemps, 17130 Montendre \
    SIRET : 98266457500015 \
    Tél. : 05 16 48 32 43 \
    Email : pitoneliane\@gmail.com
  ]),
  card([
    #text(font: serif, size: 9.4pt, weight: "bold", fill: red, tracking: 0.45pt)[CLIENT]
    #v(3.75pt)
    #set par(leading: row-leading)
    *Nom / société :* #data.client.name \
    #if data.client.address != "" [
      *Adresse :* #data.client.address \
    ]
    #if data.client.business-id != "" [
      *Identifiant :* #data.client.business-id \
    ]
    #if data.client.email != "" [
      *Email :* #data.client.email \
    ]
    #if data.client.phone != "" [
      *Tél. :* #data.client.phone \
    ]
    #if data.client.billing-address != "" [*Adresse de facturation :* #data.client.billing-address]
  ]),
)

#v(6.75pt)
#text(font: serif, size: 10.5pt, weight: "bold", fill: red)[Détail de la prestation]
#v(2.25pt)
#line(length: 100%, stroke: 0.75pt + gold)
#v(3.75pt)

#let columns = (58%, 14%, 14%, 14%)
#let detail-header = table(
  columns: columns,
  inset: (x: 6.75pt, y: 6.35pt),
  fill: red,
  stroke: none,
  text(fill: white, weight: "bold")[Désignation],
  table.cell(align: right)[#text(fill: white, weight: "bold")[Qté]],
  table.cell(align: right)[#text(fill: white, weight: "bold")[P.U. HT]],
  table.cell(align: right)[#text(fill: white, weight: "bold")[Montant HT]],
)

#let group-table(group) = table(
  columns: columns,
  inset: 0pt,
  stroke: none,
  table.header(
    table.cell(
      colspan: 4,
      fill: pink,
      inset: (x: 6.75pt, y: cell-inset-y),
      stroke: (bottom: 0.4pt + rule),
    )[#text(font: serif, weight: "bold", tracking: 0.35pt)[#group.name]],
  ),
  ..group.lines.map(line => {
    let background = if line.alternate { cream } else { white }
    let cell = (body, alignment: left) => table.cell(
      align: alignment,
      fill: background,
      inset: (x: 6.75pt, y: cell-inset-y),
      stroke: (bottom: 0.4pt + rule),
      body,
    )
    (
      cell([#line.description]),
      cell([#line.quantity], alignment: right),
      cell([#line.unit-price], alignment: right),
      cell([#line.amount], alignment: right),
    )
  }).flatten(),
)

#table(
  columns: (1fr,),
  inset: 0pt,
  stroke: none,
  table.header(table.cell(inset: 0pt, stroke: none, detail-header)),
  ..data.groups.map(group => table.cell(
    // ponytail: 12 rows fit a page; split larger groups until Typst supports avoid-with-fallback.
    breakable: group.lines.len() > 12,
    inset: 0pt,
    stroke: none,
    group-table(group),
  )),
)

#v(5.25pt)
// `.totals { width: 62%; max-width: 300px }`: on a 190 mm content width the cap
// wins, so the column is a flat 225pt against the right edge. It has to be a
// sized column and not `align(right)` — the rows shrink-wrapped their content
// otherwise, which pulled the labels away from the left edge of the block.
// The gutter only shows on the invoice, whose total label is long enough to run
// into the amount; the flex row it comes from keeps them apart by wrapping.
#let total-row(label, value) = grid(columns: (1fr, auto), column-gutter: 8pt, label, value)
#grid(
  columns: (1fr, 225pt),
  [],
  [
    #pad(x: 7.5pt, y: 2.25pt, total-row([Total HT], [#data.total]))
    #pad(x: 7.5pt, y: 2.25pt, total-row([TVA (0 %)], [0,00 €]))
    #pad(x: 7.5pt, y: 2.25pt)[
      #text(size: 7.9pt, style: "italic", fill: gray)[TVA non applicable, art. 293 B du CGI]
    ]
    #v(4.5pt)
    #block(
      width: 100%,
      fill: cream,
      stroke: 1.5pt + red,
      radius: 3pt,
      inset: (x: 7.5pt, y: 4.5pt),
    )[
      #total-row(
        text(font: serif, size: 10.5pt, weight: "bold", fill: red)[#data.total-label],
        text(font: serif, size: 10.5pt, weight: "bold", fill: red)[#data.total],
      )
    ]
  ],
)

#v(6.75pt)
#if data.quote [
  #card([
    #text(font: serif, size: 9.4pt, weight: "bold", fill: red)[Conditions]
    #v(3pt)
    // `.conditions ul { padding-left: 18px }` — the marker hangs in that padding.
    #set list(indent: 14.4pt)
    #set par(leading: row-leading)
    - *Offre valable jusqu'au :* #data.validity-end.
    - *Règlement :* #if data.payment-terms == "" [par virement la veille de la récupération (#data.event-date), ou en espèces le jour même] else [#data.payment-terms].
    - Établissement du présent devis : *gratuit*.
  ])

  #v(6.75pt)
  *Devis à retourner daté et signé, reçu avant exécution de la prestation.*
  #v(4.5pt)
  #grid(
    columns: (1fr, 1fr),
    column-gutter: 12pt,
    block(width: 100%, height: 45pt, stroke: 0.75pt + gold, radius: 3pt, inset: 7.5pt)[
      #text(font: serif, size: 9pt, weight: "bold", fill: red)[L'émetteur] \
      Variété de Saveurs \
      #text(size: 7.5pt, style: "italic", fill: gray)[Date et signature :]
    ],
    block(width: 100%, height: 45pt, stroke: 0.75pt + gold, radius: 3pt, inset: 7.5pt)[
      #text(font: serif, size: 9pt, weight: "bold", fill: red)[Le client] \
      #text(size: 7.5pt, style: "italic", fill: gray)[Mention manuscrite « Bon pour accord », date et signature :]
    ],
  )
] else [
  #card([
    #text(font: serif, size: 9.4pt, weight: "bold", fill: red)[Règlement]
    #v(3pt)
    #set par(leading: row-leading)
    *Montant à régler :* #data.total \
    *Échéance :* #data.event-date \
    *Conditions de paiement :* #data.payment-terms. \
    *Moyens de paiement :* virement bancaire ou espèces. \
    *IBAN :* FR76 4061 8804 4500 0405 9333 017 (Variété de Saveurs)
    #if data.professional [
      #v(3pt)
      #text(size: 7.5pt, fill: gray)[Pénalités de retard : taux d'intérêt de la BCE majoré de 10 points ; indemnité forfaitaire pour frais de recouvrement : 40 €. Pas d'escompte pour paiement anticipé.]
    ]
  ])

  #v(9pt)
  #align(center)[
    #text(font: serif, size: 9pt, style: "italic", fill: gray)[
      Nous restons à votre disposition pour toute question — merci encore de votre confiance.
    ]
  ]
]

#v(6.75pt)
#line(length: 100%, stroke: 0.75pt + gold)
#v(4.5pt)
#align(center)[
  #text(size: 7.1pt, fill: gray)[
    // `.footer { line-height: 1.5 }`, minus the cap height like everywhere else.
    #set par(leading: 0.812em)
    *Variété de Saveurs* - 2 impasse du printemps, 17130 Montendre • SIRET : 98266457500015 \
    Tél. : 05 16 48 32 43 • pitoneliane\@gmail.com • Facebook : Variété de Saveurs
  ]
]
