# TextField

A text field. Its label sits on the left in a column `labelWidth` wide (90px), or, with `labelHidden`, inside the empty field as its placeholder.

- Props: `label`, `value`, `placeholder`, `state` (`normal`, `focused`, `off`, `error`), `secret`, `note`, `error`, `labelHidden`, `labelWidth`, `row`.
- A placeholder never repeats the label and never ends in an ellipsis. With a visible label, leave the empty field blank.
- The line under the field starts at the field's left edge: `note` in `text-secondary`, `error` in `danger` with a `danger` ring. Say what is wrong and what to do: "Not a valid address."
- Focus draws a 1.5px `accent` ring. Fill `field`, `field-hover` under the pointer; no outline. Text in `small`.
- `row` draws it as one row of a `Box`, with the row's inset and height.
