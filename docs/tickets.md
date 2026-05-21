# Tickets

## Ticket Structure

Title (status)
User story;
As a
Id Like to
So that

Context: (why)
Description: (what)
Requirements: (how)

Acceptance criteria (serves as basis for unit tests)
Given
When
Then

---

## Active Tickets

### Connect to Music Source Wizard (To Do)
User story;
As a user
Id Like to easily connect my music source via a wizard
So that I can start listening to my music library without complicated setup

Context: (why)
The app needs access to a WebDAV server where the user's music is stored. The setup process can be technical, so a guided wizard will help users correctly input their credentials (like an app password) without frustration.

Description: (what)
Implement a UI wizard that guides the user through connecting to their WebDAV music source. The wizard should prompt for the server URL, username, and the app password, providing clear instructions along the way.

Requirements: (how)
- Create a new UI scene for the connection wizard.
- Provide input fields for Base URL, Username, and App Password.
- Include helper text or tooltips explaining what an App Password is and where to find the WebDAV details.
- Add a "Test Connection" button to verify credentials before saving.
- Securely store the provided credentials.

Acceptance criteria (serves as basis for unit tests)
Given the user is on the setup screen
When they choose to connect a music source and follow the wizard steps
Then they are prompted for WebDAV credentials (URL, username, app password) and can successfully verify and save their connection.
