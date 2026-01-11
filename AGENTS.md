## PRD Mainteinance Criteria
1. All features currently implemented should be described in [PRD.md](PRD.md). The ideal PRD is one that accurately reflects the current state of the product, is easy to read, and serves as a reliable reference for reproducing the product behavior.
2. When working, please keep PRD up-to-date. I.e., if you add a new feature, please update the PRD accordingly. If you remove a feature, please remove it from the PRD.
3. When editing PRD, please make your changes integrated in the original text, rather than appending new text at the end. You should keep it as it exists on the beginning of this document.
4. Keep the document clean and easy to read, but do not hesitate to add necessary details to clarify the design. Record all important bussines logic in PRD.
5. Make less but more accurate changes. Avoid making trivial changes that do not add value.

## Programming Style
- Prefer to use functional style over object-oriented style when possible.
- Pay attention to data flow and state management. Prefer immutable data structures when possible.
- Write modular code. Break down large functions into smaller, reusable functions.
- Keep related data and functions together. Prefer to use AoS (Array of Structures) over SoA (Structure of Arrays) when it improves code clarity.
- Keep performance in mind, but do not sacrifice code readability for minor performance gains.
