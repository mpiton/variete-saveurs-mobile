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

#set document(title: "DEVIS n° " + str(data.number) + " - Variété de Saveurs")
#set page(
  paper: "a4",
  margin: (top: 10mm, right: 10mm, bottom: 14mm, left: 10mm),
  footer: context align(center)[Page #counter(page).display("1 / 1", both: true)],
)
#set text(font: sans, size: 8.25pt, fill: brown, lang: "fr")
#set par(leading: 2.6pt)

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
    #text(font: serif, size: 25.5pt, weight: "bold", fill: red, tracking: 2.25pt)[DEVIS]
    #v(2pt)
    #text(font: serif, size: 9pt, style: "italic", fill: gray)[Offre gratuite et sans engagement]
    #v(4.5pt)
    #text(size: 8.25pt)[
      *N° de devis :* #data.number \
      *Date d'émission :* #data.issue-date \
      *Date de l'événement :* #data.event-date \
      *Validité de l'offre :* jusqu'au #data.validity-end
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
    #text(size: 9.4pt, weight: "bold")[Variété de Saveurs] \
    2 impasse du printemps, 17130 Montendre \
    SIRET : 98266457500015 \
    Tél. : 05 16 48 32 43 \
    Email : pitoneliane\@gmail.com
  ]),
  card([
    #text(font: serif, size: 9.4pt, weight: "bold", fill: red, tracking: 0.45pt)[CLIENT]
    #v(3.75pt)
    *Nom / société :* #data.client.name \
    *Adresse :* #data.client.address \
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
  inset: (x: 6.75pt, y: 5.25pt),
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
  table.cell(
    colspan: 4,
    fill: pink,
    inset: (x: 6.75pt, y: 3.75pt),
    stroke: (bottom: 0.4pt + rule),
  )[#text(font: serif, weight: "bold", tracking: 0.35pt)[#group.name]],
  ..group.lines.map(line => {
    let background = if line.alternate { cream } else { white }
    let cell = (body, alignment: left) => table.cell(
      align: alignment,
      fill: background,
      inset: (x: 6.75pt, y: 4.125pt),
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
    breakable: false,
    inset: 0pt,
    stroke: none,
    group-table(group),
  )),
)

#v(5.25pt)
#align(right)[
  #block(width: 62%)[
    #grid(columns: (1fr, auto), column-gutter: 8pt, inset: (x: 7.5pt, y: 2.25pt), [Total HT], [#data.total])
    #grid(columns: (1fr, auto), column-gutter: 8pt, inset: (x: 7.5pt, y: 2.25pt), [TVA (0 %)], [0,00 €])
    #text(size: 7.9pt, style: "italic", fill: gray)[TVA non applicable, art. 293 B du CGI]
    #v(4.5pt)
    #block(
      width: 100%,
      fill: cream,
      stroke: 1.5pt + red,
      radius: 3pt,
      inset: (x: 7.5pt, y: 4.5pt),
    )[
      #grid(
        columns: (1fr, auto),
        column-gutter: 8pt,
        text(font: serif, size: 10.5pt, weight: "bold", fill: red)[Total du devis],
        text(font: serif, size: 10.5pt, weight: "bold", fill: red)[#data.total],
      )
    ]
  ]
]

#v(6.75pt)
#card([
  #text(font: serif, size: 9.4pt, weight: "bold", fill: red)[Conditions]
  #v(3pt)
  - *Offre valable jusqu'au :* #data.validity-end.
  - *Règlement :* par virement la veille de la récupération (#data.event-date), ou en espèces le jour même.
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

#v(6.75pt)
#line(length: 100%, stroke: 0.75pt + gold)
#v(4.5pt)
#align(center)[
  #text(size: 7.1pt, fill: gray)[
    *Variété de Saveurs* - 2 impasse du printemps, 17130 Montendre • SIRET : 98266457500015 \
    Tél. : 05 16 48 32 43 • pitoneliane\@gmail.com • Facebook : Variété de Saveurs
  ]
]
