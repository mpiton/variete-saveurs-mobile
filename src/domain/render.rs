use std::fmt::Write;

use chrono::NaiveDate;
use html_escape::encode_text;

use super::models::{DocumentInput, DocumentKind};
use super::money::format_eur;

const CSS: &str = include_str!("../../templates/document.css");
const LOGO_BYTES: &[u8] = include_bytes!("../../templates/logo.png");

pub fn render_document_html(input: &DocumentInput, number: i64) -> String {
    let (title, number_label, nature) = match &input.kind {
        DocumentKind::Quote => ("DEVIS", "N° de devis", "Offre gratuite et sans engagement"),
        DocumentKind::Invoice => ("FACTURE", "N° de facture", "Merci de votre confiance"),
    };
    let is_quote = matches!(&input.kind, DocumentKind::Quote);
    let issue_date = escape(&format_date(&input.issue_date));
    let event_date = escape(&format_date(&input.event_date));
    let validity_end = validity_end_date(&input.issue_date);
    let payment_terms = escape(&input.payment_terms);
    let total = format_eur(input.total_cents());
    let total_label = if is_quote {
        "Total du devis".to_string()
    } else {
        format!("Total net à payer avant le {event_date}")
    };
    let logo_base64 = base64_encode(LOGO_BYTES);

    let mut html = String::new();
    write!(
        html,
        r#"<!DOCTYPE html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} n° {number} - Variété de Saveurs</title>
<style>
{CSS}
</style>
</head>
<body>
<div class="page">
  <header class="header">
    <div>
      <img class="logo" src="data:image/png;base64,{logo_base64}" alt="Logo Variété de Saveurs">
      <div class="brand-name">Variété de Saveurs</div>
    </div>
    <div class="document-box">
      <h1>{title}</h1>
      <div class="nature">{nature}</div>
      <div class="meta">
        <div><b>{number_label} :</b> {number}</div>
        <div><b>Date d'émission :</b> {issue_date}</div>
        <div><b>Date de l'événement :</b> {event_date}</div>
"#
    )
    .expect("writing to String cannot fail");

    if is_quote {
        let validity = match &validity_end {
            Some(end) => format!("jusqu'au {end}"),
            None => "30 jours".to_string(),
        };
        writeln!(
            html,
            r#"        <div><b>Validité de l'offre :</b> {validity}</div>"#
        )
        .expect("writing to String cannot fail");
    } else {
        writeln!(
            html,
            r#"        <div><b>Conditions de paiement :</b> {payment_terms}</div>"#
        )
        .expect("writing to String cannot fail");
    }

    write!(
        html,
        r#"      </div>
    </div>
  </header>

  <section class="parties">
    <div class="party">
      <p class="title">Émetteur</p>
      <div class="name">Variété de Saveurs</div>
      <div>2 impasse du printemps, 17130 Montendre</div>
      <div>SIRET : 98266457500015</div>
      <div>Tél. : 05 16 48 32 43</div>
      <div>Email : pitoneliane@gmail.com</div>
    </div>
    <div class="party">
      <p class="title">Client</p>
      <div><b>Nom / société :</b> {client_name}</div>
      <div><b>Adresse :</b> {client_address}</div>
"#,
        client_name = escape(&input.client.name),
        client_address = escape(&input.client.address)
    )
    .expect("writing to String cannot fail");

    if let Some(business_id) = non_empty(input.client.business_id.as_deref()) {
        writeln!(
            html,
            r#"      <div><b>Identifiant :</b> {}</div>"#,
            escape(business_id)
        )
        .expect("writing to String cannot fail");
    }
    if let Some(email) = non_empty(input.client.email.as_deref()) {
        writeln!(html, r#"      <div><b>Email :</b> {}</div>"#, escape(email))
            .expect("writing to String cannot fail");
    }
    if let Some(phone) = non_empty(input.client.phone.as_deref()) {
        writeln!(html, r#"      <div><b>Tél. :</b> {}</div>"#, escape(phone))
            .expect("writing to String cannot fail");
    }
    if let Some(billing_address) = non_empty(input.client.billing_address.as_deref()) {
        writeln!(
            html,
            r#"      <div><b>Adresse de facturation :</b> {}</div>"#,
            escape(billing_address)
        )
        .expect("writing to String cannot fail");
    }

    write!(
        html,
        r#"    </div>
  </section>

  <h2 class="section-title">Détail de la prestation</h2>
  <table class="lines">
    <thead>
      <tr>
        <th class="desc">Désignation</th>
        <th class="num">Qté</th>
        <th class="num">P.U. HT</th>
        <th class="num">Montant HT</th>
      </tr>
    </thead>
"#
    )
    .expect("writing to String cannot fail");

    write_line_rows(&mut html, input);

    write!(
        html,
        r#"  </table>

  <div class="totals-wrap">
    <div class="totals">
      <div class="row"><span>Total HT</span><span>{total}</span></div>
      <div class="row"><span>TVA (0 %)</span><span>{vat}</span></div>
      <div class="row sub">TVA non applicable, art. 293 B du CGI</div>
      <div class="ttc"><div class="row"><span>{total_label}</span><span>{total}</span></div></div>
    </div>
  </div>
"#,
        vat = format_eur(0)
    )
    .expect("writing to String cannot fail");

    if is_quote {
        write_quote_conditions_and_signature(&mut html, &validity_end, &issue_date, &event_date);
    } else {
        write_invoice_payment(&mut html, input, &event_date, &payment_terms, &total);
    }

    write!(
        html,
        r#"
  <footer class="footer">
    <b>Variété de Saveurs</b> - 2 impasse du printemps, 17130 Montendre &nbsp;•&nbsp; SIRET : 98266457500015<br>
    Tél. : 05 16 48 32 43 &nbsp;•&nbsp; pitoneliane@gmail.com &nbsp;•&nbsp; Facebook : Variété de Saveurs
  </footer>
</div>
</body>
</html>
"#
    )
    .expect("writing to String cannot fail");

    html
}

fn write_line_rows(html: &mut String, input: &DocumentInput) {
    let mut rendered = vec![false; input.lines.len()];
    let mut rendered_index = 0_usize;

    for (index, line) in input.lines.iter().enumerate() {
        if rendered[index] {
            continue;
        }

        let Some(group) = non_empty(line.group.as_deref()) else {
            rendered[index] = true;
            html.push_str("    <tbody>\n");
            write_line_item_row(html, line, rendered_index);
            html.push_str("    </tbody>\n");
            rendered_index += 1;
            continue;
        };

        // One tbody per group so print keeps the label with its items
        // (break-inside: avoid). ponytail: a group taller than one page still
        // splits and loses its label on the next page; repeat "(suite)" rows
        // if orders ever grow that large.
        writeln!(
            html,
            r#"    <tbody class="group-block">
      <tr class="group"><td colspan="4">{}</td></tr>"#,
            escape(group)
        )
        .expect("writing to String cannot fail");

        for (grouped_index, grouped_line) in input.lines.iter().enumerate() {
            if rendered[grouped_index] {
                continue;
            }
            if non_empty(grouped_line.group.as_deref()) == Some(group) {
                rendered[grouped_index] = true;
                write_line_item_row(html, grouped_line, rendered_index);
                rendered_index += 1;
            }
        }
        html.push_str("    </tbody>\n");
    }
}

fn write_line_item_row(html: &mut String, line: &super::models::LineInput, index: usize) {
    let alt_class = if index % 2 == 1 { " alt" } else { "" };
    writeln!(
        html,
        r#"      <tr class="item{alt_class}"><td class="desc">{description}</td><td class="num">{quantity}</td><td class="num">{unit_price}</td><td class="num">{amount}</td></tr>"#,
        description = escape(&line.description),
        quantity = line.quantity,
        unit_price = format_eur(line.unit_price_cents),
        amount = format_eur(line.amount_cents())
    )
    .expect("writing to String cannot fail");
}

fn write_quote_conditions_and_signature(
    html: &mut String,
    validity_end: &Option<String>,
    issue_date: &str,
    event_date: &str,
) {
    let validity_line = match validity_end {
        Some(end) => format!("<b>Offre valable jusqu'au :</b> {end}."),
        None => format!("<b>Validité de l'offre :</b> 30 jours à compter du {issue_date}."),
    };
    write!(
        html,
        r#"
  <section class="conditions">
    <div class="title">Conditions</div>
    <ul>
      <li>{validity_line}</li>
      <li><b>Règlement :</b> par virement la veille de la récupération ({event_date}), ou en espèces le jour même.</li>
      <li>Établissement du présent devis : <b>gratuit</b>.</li>
    </ul>
  </section>

  <section class="agreement">
    <div class="intro">Devis à retourner daté et signé, reçu avant exécution de la prestation.</div>
    <div class="signatures">
      <div class="sign">
        <div class="role">L'émetteur</div>
        <div>Variété de Saveurs</div>
        <div class="mention">Date et signature :</div>
      </div>
      <div class="sign">
        <div class="role">Le client</div>
        <div class="mention">Mention manuscrite « Bon pour accord »,<br>date et signature :</div>
      </div>
    </div>
  </section>
"#
    )
    .expect("writing to String cannot fail");
}

fn write_invoice_payment(
    html: &mut String,
    input: &DocumentInput,
    event_date: &str,
    payment_terms: &str,
    total: &str,
) {
    write!(
        html,
        r#"
  <section class="payment-block">
    <div class="title">Règlement</div>
    <div><b>Montant à régler :</b> {total}</div>
    <div><b>Échéance :</b> {event_date}</div>
    <div><b>Conditions de paiement :</b> {payment_terms}.</div>
    <div><b>Moyens de paiement :</b> virement bancaire ou espèces.</div>
    <div><b>IBAN :</b> FR76 4061 8804 4500 0405 9333 017 (Variété de Saveurs)</div>
"#
    )
    .expect("writing to String cannot fail");

    if matches!(input.client.kind, super::models::ClientKind::Professional) {
        html.push_str(
            r#"    <div class="mentions">Pénalités de retard : taux d'intérêt de la BCE majoré de 10 points ; indemnité forfaitaire pour frais de recouvrement : 40 €. Pas d'escompte pour paiement anticipé.</div>
"#,
        );
    }

    html.push_str("  </section>\n");
    html.push_str(
        r#"
  <p class="closing">Nous restons à votre disposition pour toute question — merci encore de votre confiance.</p>
"#,
    );
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn format_date(value: &str) -> String {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| date.format("%d/%m/%Y").to_string())
        .unwrap_or_else(|_| value.to_string())
}

fn validity_end_date(issue_date: &str) -> Option<String> {
    NaiveDate::parse_from_str(issue_date, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.checked_add_days(chrono::Days::new(30)))
        .map(|date| date.format("%d/%m/%Y").to_string())
}

fn escape(value: &str) -> String {
    encode_text(value).to_string()
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        encoded.push(TABLE[(b0 >> 2) as usize] as char);
        encoded.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);

        if chunk.len() > 1 {
            encoded.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }

        if chunk.len() > 2 {
            encoded.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            encoded.push('=');
        }
    }

    encoded
}

#[cfg(test)]
mod tests {
    use super::render_document_html;
    use crate::domain::models::{ClientInput, ClientKind, DocumentInput, DocumentKind, LineInput};

    fn document(kind: DocumentKind) -> DocumentInput {
        DocumentInput {
            kind,
            issue_date: "2026-07-01".to_string(),
            event_date: "2026-07-19".to_string(),
            payment_terms: "à réception".to_string(),
            client: ClientInput {
                kind: ClientKind::Professional,
                name: "Client & Co".to_string(),
                address: "1 rue <test>".to_string(),
                email: Some("contact@example.test".to_string()),
                phone: Some("05 00 00 00 00".to_string()),
                business_id: Some("SIRET <123>".to_string()),
                billing_address: None,
            },
            lines: vec![LineInput {
                group: Some("Salé".to_string()),
                description: "Mini Burgers".to_string(),
                quantity: 50,
                unit_price_cents: 85,
            }],
        }
    }

    #[test]
    fn renders_quote_with_escaped_client_and_totals() {
        let html = render_document_html(&document(DocumentKind::Quote), 9);

        assert!(html.contains("DEVIS"));
        assert!(html.contains("N° de devis"));
        assert!(html.contains("Client &amp; Co"));
        assert!(html.contains("1 rue &lt;test&gt;"));
        assert!(html.contains("42,50 €"));
        assert!(html.contains("Bon pour accord"));
        assert!(html.contains("Offre gratuite et sans engagement"));
        assert!(html.contains("Total du devis"));
        assert!(!html.contains("Total net à payer"));
        assert!(html.contains("jusqu'au 31/07/2026"));
        assert_eq!(html.matches("TVA non applicable").count(), 1);
    }

    #[test]
    fn renders_invoice_payment_terms() {
        let html = render_document_html(&document(DocumentKind::Invoice), 12);

        assert!(html.contains("FACTURE"));
        assert!(html.contains("N° de facture"));
        assert!(html.contains("Merci de votre confiance"));
        assert!(html.contains("Total net à payer avant le 19/07/2026"));
        assert!(!html.contains("Total du devis"));
        assert!(html.contains("Règlement"));
        assert!(html.contains("Échéance :</b> 19/07/2026"));
        assert!(html.contains("Conditions de paiement :</b> à réception."));
        assert!(html.contains("IBAN :</b> FR76 4061 8804 4500 0405 9333 017"));
        assert!(!html.contains("Facture payable"));
        assert!(html.contains("Pénalités de retard"));
        assert_eq!(html.matches("TVA non applicable").count(), 1);
        assert!(html.contains("Nous restons à votre disposition"));
    }

    #[test]
    fn individual_invoice_omits_b2b_late_payment_mentions() {
        let mut doc = document(DocumentKind::Invoice);
        doc.client.kind = ClientKind::Individual;
        doc.client.business_id = None;

        let html = render_document_html(&doc, 12);

        assert!(html.contains("FACTURE"));
        assert!(!html.contains("Pénalités de retard"));
    }

    #[test]
    fn invoice_renders_payment_block_once_without_conditions_duplicate() {
        let html = render_document_html(&document(DocumentKind::Invoice), 12);

        assert_eq!(html.matches("Moyens de paiement").count(), 1);
        assert!(!html.contains(r#"<section class="conditions">"#));
    }

    #[test]
    fn quote_does_not_render_bank_details() {
        let html = render_document_html(&document(DocumentKind::Quote), 9);

        assert!(html.contains("DEVIS"));
        assert!(!html.contains("IBAN :"));
        assert!(!html.contains(r#"<section class="payment-block">"#));
    }

    #[test]
    fn invoice_does_not_render_quote_signature() {
        let html = render_document_html(&document(DocumentKind::Invoice), 12);

        assert!(html.contains("FACTURE"));
        assert!(!html.contains("Bon pour accord"));
    }

    #[test]
    fn renders_each_line_group_once_even_when_lines_are_interleaved() {
        let mut doc = document(DocumentKind::Quote);
        doc.lines = vec![
            LineInput {
                group: Some("Sucré".to_string()),
                description: "Mini Brochettes de fruits".to_string(),
                quantity: 1,
                unit_price_cents: 85,
            },
            LineInput {
                group: Some("Salé".to_string()),
                description: "Mini Pizzas".to_string(),
                quantity: 1,
                unit_price_cents: 85,
            },
            LineInput {
                group: Some("Sucré".to_string()),
                description: "Mini Cakes".to_string(),
                quantity: 1,
                unit_price_cents: 85,
            },
        ];

        let html = render_document_html(&doc, 9);

        assert_eq!(
            html.matches(r#"<tr class="group"><td colspan="4">Sucré</td></tr>"#)
                .count(),
            1
        );
        assert_eq!(
            html.matches(r#"<tr class="group"><td colspan="4">Salé</td></tr>"#)
                .count(),
            1
        );
        assert_eq!(html.matches(r#"<tbody class="group-block">"#).count(), 2);
    }

    #[test]
    fn escapes_all_user_text_in_html() {
        let mut doc = document(DocumentKind::Invoice);
        doc.issue_date = "<issue>".to_string();
        doc.event_date = "<event>".to_string();
        doc.payment_terms = "<payment>".to_string();
        doc.client.name = "<script>alert(1)</script>".to_string();
        doc.client.address = "<address>".to_string();
        doc.client.email = Some("<email>".to_string());
        doc.client.phone = Some("<phone>".to_string());
        doc.client.business_id = Some("<business>".to_string());
        doc.client.billing_address = Some("<billing>".to_string());
        doc.lines[0].group = Some("<group>".to_string());
        doc.lines[0].description = "<description>".to_string();

        let html = render_document_html(&doc, 12);

        for raw in [
            "<issue>",
            "<event>",
            "<payment>",
            "<script>",
            "<address>",
            "<email>",
            "<phone>",
            "<business>",
            "<billing>",
            "<group>",
            "<description>",
        ] {
            assert!(!html.contains(raw), "unescaped user content: {raw}");
        }
        for escaped in [
            "&lt;issue&gt;",
            "&lt;event&gt;",
            "&lt;payment&gt;",
            "&lt;script&gt;alert(1)&lt;/script&gt;",
            "&lt;address&gt;",
            "&lt;email&gt;",
            "&lt;phone&gt;",
            "&lt;business&gt;",
            "&lt;billing&gt;",
            "&lt;group&gt;",
            "&lt;description&gt;",
        ] {
            assert!(html.contains(escaped), "missing escaped content: {escaped}");
        }
    }

    #[test]
    fn embeds_unchanged_template_logo_and_pagination() {
        let html = render_document_html(&document(DocumentKind::Quote), 9);

        assert!(html.contains("data:image/png;base64,iVBORw0KGgo"));
        assert!(html.contains("@page"));
        assert!(html.contains(r#"content: "Page " counter(page) " / " counter(pages);"#));
        assert!(html.contains("tbody.group-block"));
    }
}
