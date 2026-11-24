import AppKit
import SwiftUI

/// Testo di un messaggio con markdown (grassetto, corsivo, elenchi in linea, codice, collegamenti) e blocchi di
/// codice con il pulsante «Copia» (ADR-0127). I collegamenti si aprono solo se sono http o https.
struct MessageContent: View {
    let text: String

    enum Segment {
        case prose(String)
        case code(language: String, body: String)
    }

    /// Divide il testo in parti normali e blocchi delimitati da tre apici inversi; un blocco non chiuso resta codice.
    static func segments(_ text: String) -> [Segment] {
        var result: [Segment] = []
        var prose: [String] = []
        var code: [String] = []
        var language = ""
        var inCode = false
        func flushProse() {
            let joined = prose.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
            if !joined.isEmpty { result.append(.prose(joined)) }
            prose = []
        }
        for line in text.components(separatedBy: "\n") {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("```") {
                if inCode {
                    result.append(.code(language: language, body: code.joined(separator: "\n")))
                    code = []
                    inCode = false
                } else {
                    flushProse()
                    language = String(trimmed.dropFirst(3)).trimmingCharacters(in: .whitespaces)
                    inCode = true
                }
            } else if inCode {
                code.append(line)
            } else {
                prose.append(line)
            }
        }
        if inCode {
            result.append(.code(language: language, body: code.joined(separator: "\n")))
        } else {
            flushProse()
        }
        return result
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(MessageContent.segments(text).enumerated()), id: \.offset) { _, segment in
                switch segment {
                case .prose(let value):
                    Text(MessageContent.attributed(value))
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                case .code(let language, let body):
                    CodeBlock(language: language, code: body)
                }
            }
        }
    }

    /// Markdown in linea; i collegamenti con schemi diversi da http e https vengono tolti.
    static func attributed(_ value: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(
            allowsExtendedAttributes: false,
            interpretedSyntax: .inlineOnlyPreservingWhitespace,
            failurePolicy: .returnPartiallyParsedIfPossible
        )
        guard var parsed = try? AttributedString(markdown: value, options: options) else {
            return AttributedString(value)
        }
        for run in parsed.runs {
            if let url = run.link, !["http", "https"].contains(url.scheme?.lowercased() ?? "") {
                parsed[run.range].link = nil
            }
        }
        return parsed
    }
}

/// Blocco di codice con intestazione e pulsante «Copia».
struct CodeBlock: View {
    let language: String
    let code: String
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language.isEmpty ? "codice" : language)
                    .font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                Spacer()
                Button {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(code, forType: .string)
                    copied = true
                    Task {
                        try? await Task.sleep(for: .seconds(1.6))
                        copied = false
                    }
                } label: {
                    Label(copied ? "Copiato" : "Copia", systemImage: copied ? "checkmark" : "doc.on.doc")
                        .font(.caption2)
                }
                .buttonStyle(.plain)
                .foregroundStyle(copied ? Color.green : Color.cyan)
            }
            .padding(.horizontal, 10).padding(.vertical, 6)
            .background(Color.white.opacity(0.05))
            ScrollView(.horizontal) {
                Text(code)
                    .font(.system(.callout, design: .monospaced))
                    .textSelection(.enabled)
                    .padding(10)
            }
        }
        .background(Color.black.opacity(0.28), in: RoundedRectangle(cornerRadius: 10))
        .overlay { RoundedRectangle(cornerRadius: 10).strokeBorder(Color.white.opacity(0.08)) }
        .animation(.snappy, value: copied)
    }
}
